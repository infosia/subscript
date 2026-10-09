/* Scratch-file provider for the native corpus tiers (§185). */
#include "subscript_runtime.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern const char* subscript_test_file_root(void);
static char* pending_path;
static subscript_rt_completion pending_endpoint;
static uint32_t pending_form;

static void fail(subscript_rt_context* ctx, subscript_rt_completion endpoint,
                 const char* message) {
    subscript_rt_complete_error(ctx, endpoint, message, strlen(message));
}

static char* full_path(const uint8_t* path, size_t length) {
    const char* root = subscript_test_file_root();
    size_t n = strlen(root);
    char* value = malloc(n + length + 2);
    if (!value) return NULL;
    memcpy(value, root, n);
    value[n] = '/';
    memcpy(value + n + 1, path, length);
    value[n + length + 1] = 0;
    return value;
}

static void finish_read(subscript_rt_context* ctx, const char* path,
                        uint32_t form, subscript_rt_completion endpoint) {
    FILE* file = fopen(path, "rb");
    if (!file) { fail(ctx, endpoint, "missing file"); return; }
    if (fseek(file, 0, SEEK_END) != 0) {
        fclose(file); fail(ctx, endpoint, "read failed"); return;
    }
    long length = ftell(file);
    if (length < 0 || fseek(file, 0, SEEK_SET) != 0) {
        fclose(file); fail(ctx, endpoint, "read failed"); return;
    }
    uint8_t* bytes = malloc(length ? (size_t)length : 1);
    if (!bytes) { fclose(file); fail(ctx, endpoint, "read failed"); return; }
    size_t actual = fread(bytes, 1, (size_t)length, file);
    int closed = fclose(file);
    if (actual != (size_t)length || closed != 0) {
        free(bytes); fail(ctx, endpoint, "read failed"); return;
    }
    subscript_rt_completion_status status = form == 0
        ? subscript_rt_complete_string(ctx, endpoint, (const char*)bytes, actual)
        : subscript_rt_complete_bytes(ctx, endpoint, bytes, actual);
    free(bytes);
    if (status == SUBSCRIPT_RT_COMPLETION_INVALID_UTF8 ||
        status == SUBSCRIPT_RT_COMPLETION_TOO_LARGE) {
        fail(ctx, endpoint, "invalid file result");
    }
}

static void read_file(subscript_rt_context* ctx, void* userdata,
                      const uint8_t* path, size_t length, uint32_t form,
                      subscript_rt_completion endpoint) {
    (void)userdata;
    char* filename = full_path(path, length);
    if (!filename) { fail(ctx, endpoint, "read failed"); return; }
    if (length == 11 && memcmp(path, "pending.txt", 11) == 0) {
        pending_path = filename;
        pending_endpoint = endpoint;
        pending_form = form;
        return;
    }
    finish_read(ctx, filename, form, endpoint);
    free(filename);
}

static void write_file(subscript_rt_context* ctx, void* userdata,
                       const uint8_t* path, size_t length,
                       const uint8_t* data, size_t data_length, uint32_t form,
                       subscript_rt_completion endpoint) {
    (void)userdata; (void)form;
    char* filename = full_path(path, length);
    FILE* file = filename ? fopen(filename, "wb") : NULL;
    free(filename);
    if (!file) { fail(ctx, endpoint, "write failed"); return; }
    /* data can be NULL when data_length is 0. */
    size_t actual = data_length ? fwrite(data, 1, data_length, file) : 0;
    int closed = fclose(file);
    if (actual == data_length && closed == 0) subscript_rt_complete_void(ctx, endpoint);
    else fail(ctx, endpoint, "write failed");
    if (pending_path) {
        finish_read(ctx, pending_path, pending_form, pending_endpoint);
        free(pending_path);
        pending_path = NULL;
    }
}

void subscript_test_files_setup(subscript_rt_context* ctx) {
    subscript_rt_file_provider provider = {
        sizeof(subscript_rt_file_provider), NULL, read_file, write_file
    };
    subscript_rt_ctx_set_file_provider(ctx, &provider);
}
