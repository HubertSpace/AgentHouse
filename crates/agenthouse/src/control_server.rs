use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use ah_control::{
    ControlCommand, ControlErrorInfo, ControlRequest, ControlResponse, ControlResult,
    ControlStreamMessage,
};
use tokio::sync::mpsc;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct QueuedControlRequest {
    pub request: ControlRequest,
    pub respond_to: std_mpsc::Sender<ControlResponse>,
}

pub fn control_socket_path() -> PathBuf {
    std::env::temp_dir().join(ah_control::DEFAULT_SOCKET_NAME)
}

pub fn start_control_server(
    requests: mpsc::UnboundedSender<QueuedControlRequest>,
) -> Result<PathBuf, ControlServerError> {
    let path = control_socket_path();
    prepare_socket_path(&path)?;

    let listener = UnixListener::bind(&path)?;
    let server_path = path.clone();
    thread::spawn(move || {
        tracing::info!(path = %server_path.display(), "AgentHouse control socket listening");
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    let requests = requests.clone();
                    thread::spawn(move || handle_connection(stream, requests));
                }
                Err(error) => {
                    tracing::warn!(?error, "failed to accept control connection");
                }
            }
        }
    });

    Ok(path)
}

fn prepare_socket_path(path: &Path) -> Result<(), ControlServerError> {
    if !path.exists() {
        return Ok(());
    }

    if UnixStream::connect(path).is_ok() {
        return Err(ControlServerError::SocketAlreadyRunning(path.to_path_buf()));
    }

    fs::remove_file(path)?;
    Ok(())
}

fn handle_connection(
    mut stream: UnixStream,
    requests: mpsc::UnboundedSender<QueuedControlRequest>,
) {
    let Ok(reader_stream) = stream.try_clone() else {
        tracing::warn!("failed to clone control stream");
        return;
    };
    let reader = BufReader::new(reader_stream);

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(?error, "failed to read control request");
                break;
            }
        };

        if is_watch_events_request(&line) {
            if let Err(error) = handle_watch_events(&line, &mut stream, &requests) {
                tracing::warn!(?error, "failed to stream control events");
            }
            break;
        }

        let response = handle_line(&line, &requests);
        if let Err(error) = write_response(&mut stream, &response) {
            tracing::warn!(?error, "failed to write control response");
            break;
        }
    }
}

fn is_watch_events_request(line: &str) -> bool {
    match serde_json::from_str::<ControlRequest>(line) {
        Ok(request) => matches!(request.command, ControlCommand::WatchEvents { .. }),
        Err(_) => false,
    }
}

fn handle_watch_events(
    line: &str,
    stream: &mut UnixStream,
    requests: &mpsc::UnboundedSender<QueuedControlRequest>,
) -> Result<(), ControlServerError> {
    let request = match serde_json::from_str::<ControlRequest>(line) {
        Ok(request) => request,
        Err(error) => {
            write_stream_message(
                stream,
                &ControlStreamMessage::Error(ControlErrorInfo::new(
                    "decode_error",
                    format!("failed to decode request: {error}"),
                )),
            )?;
            return Ok(());
        }
    };

    let ControlCommand::WatchEvents {
        mut since_sequence,
        limit,
    } = request.command
    else {
        return Ok(());
    };
    let limit = limit.unwrap_or(100).min(500);

    loop {
        let response = queue_request(
            ControlRequest {
                id: request.id.clone(),
                command: ControlCommand::ListEvents {
                    since_sequence,
                    limit: Some(limit),
                },
            },
            requests,
        );

        let mut emitted = 0usize;
        match response.result {
            ControlResult::Events { events } => {
                for event in events {
                    since_sequence = Some(event.sequence);
                    write_stream_message(stream, &ControlStreamMessage::Event(event))?;
                    emitted += 1;
                }
            }
            ControlResult::Error(error) => {
                write_stream_message(stream, &ControlStreamMessage::Error(error))?;
                return Ok(());
            }
            other => {
                write_stream_message(
                    stream,
                    &ControlStreamMessage::Error(ControlErrorInfo::new(
                        "unexpected_result",
                        format!("unexpected watch_events result: {other:?}"),
                    )),
                )?;
                return Ok(());
            }
        }

        if emitted == 0 {
            write_stream_message(
                stream,
                &ControlStreamMessage::Heartbeat {
                    last_sequence: since_sequence.unwrap_or_default(),
                },
            )?;
            thread::sleep(Duration::from_millis(500));
        }
    }
}

fn write_stream_message(
    stream: &mut UnixStream,
    message: &ControlStreamMessage,
) -> Result<(), ControlServerError> {
    serde_json::to_writer(&mut *stream, message)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn queue_request(
    request: ControlRequest,
    requests: &mpsc::UnboundedSender<QueuedControlRequest>,
) -> ControlResponse {
    let id = request.id.clone();
    let (respond_to, response_rx) = std_mpsc::channel();
    if requests
        .send(QueuedControlRequest {
            request,
            respond_to,
        })
        .is_err()
    {
        return control_error(id, "server_closed", "control request queue is closed");
    }

    match response_rx.recv_timeout(RESPONSE_TIMEOUT) {
        Ok(response) => response,
        Err(std_mpsc::RecvTimeoutError::Timeout) => {
            control_error(id, "timeout", "control request timed out")
        }
        Err(std_mpsc::RecvTimeoutError::Disconnected) => {
            control_error(id, "server_closed", "control response channel closed")
        }
    }
}

fn handle_line(
    line: &str,
    requests: &mpsc::UnboundedSender<QueuedControlRequest>,
) -> ControlResponse {
    let request = match serde_json::from_str::<ControlRequest>(line) {
        Ok(request) => request,
        Err(error) => {
            return control_error(
                "",
                "decode_error",
                format!("failed to decode request: {error}"),
            );
        }
    };

    queue_request(request, requests)
}

fn write_response(
    stream: &mut UnixStream,
    response: &ControlResponse,
) -> Result<(), ControlServerError> {
    serde_json::to_writer(&mut *stream, response)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn control_error(
    id: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> ControlResponse {
    ControlResponse {
        id: id.into(),
        result: ControlResult::Error(ControlErrorInfo::new(code, message)),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ControlServerError {
    #[error("control socket is already running: {0}")]
    SocketAlreadyRunning(PathBuf),
    #[error("control socket io failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("control socket serialization failed: {0}")]
    Serde(#[from] serde_json::Error),
}
