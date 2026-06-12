// C shim for libdispatch functions that are macros or not directly
// accessible from Rust's extern "C" (e.g., dispatch_get_main_queue is a macro).

#include <dispatch/dispatch.h>

void* ah_dispatch_get_main_queue(void) {
    return (void*)dispatch_get_main_queue();
}

void ah_dispatch_sync_f(void* queue, void* context, void (*work)(void*)) {
    dispatch_sync_f((dispatch_queue_t)queue, context, work);
}

void ah_dispatch_async_f(void* queue, void* context, void (*work)(void*)) {
    dispatch_async_f((dispatch_queue_t)queue, context, work);
}
