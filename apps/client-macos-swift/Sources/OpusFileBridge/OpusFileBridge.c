#include "OpusFileBridge.h"
#include <opus/opusfile.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdatomic.h>
#include <string.h>

struct GdayOpusFile { OggOpusFile *decoder; FILE *file; uint64_t bytes_read; };
static int read_file(void *source, unsigned char *data, int length) {
    GdayOpusFile *s = source;
    size_t count = fread(data, 1, (size_t)length, s->file);
    s->bytes_read += count;
    return count == 0 && ferror(s->file) ? -1 : (int)count;
}
static int seek_file(void *source, opus_int64 offset, int whence) {
    return fseeko(((GdayOpusFile *)source)->file, (off_t)offset, whence);
}
static opus_int64 tell_file(void *source) { return ftello(((GdayOpusFile *)source)->file); }
static int close_file(void *source) { return fclose(((GdayOpusFile *)source)->file); }
GdayOpusFile *gday_opus_open(const char *path, int *error) {
    GdayOpusFile *s = calloc(1, sizeof(*s));
    if (!s) { *error = OP_EFAULT; return NULL; }
    s->file = fopen(path, "rb");
    if (!s->file) { *error = OP_EREAD; free(s); return NULL; }
    OpusFileCallbacks callbacks = {read_file, seek_file, tell_file, close_file};
    s->decoder = op_open_callbacks(s, &callbacks, NULL, 0, error);
    if (!s->decoder) { fclose(s->file); free(s); return NULL; }
    // Preserve per-track channel identity; chained streams need a separate UI
    // and format-transition policy. Ordinary mono/stereo recordings use one link.
    int channels = op_channel_count(s->decoder, 0);
    if (op_link_count(s->decoder) != 1 || channels < 1 || channels > 2 || op_pcm_total(s->decoder, -1) <= 0) {
        *error = OP_EIMPL; gday_opus_close(s); return NULL;
    }
    return s;
}
void gday_opus_close(GdayOpusFile *s) { if (s) { op_free(s->decoder); free(s); } }
int gday_opus_channels(GdayOpusFile *s) { return op_channel_count(s->decoder, 0); }
int64_t gday_opus_frames(GdayOpusFile *s) { return op_pcm_total(s->decoder, -1); }
int64_t gday_opus_position(GdayOpusFile *s) { return op_pcm_tell(s->decoder); }
uint64_t gday_opus_bytes_read(GdayOpusFile *s) { return s->bytes_read; }
int gday_opus_seek(GdayOpusFile *s, int64_t frame) { return op_pcm_seek(s->decoder, frame); }
int gday_opus_read(GdayOpusFile *s, float *samples, int capacity) { return op_read_float(s->decoder, samples, capacity, NULL); }

struct GdayPlaybackRing {
    uint32_t tracks, capacity;
    float *samples;
    _Atomic uint64_t written, consumed, underruns;
    _Atomic uint32_t audible;
};
GdayPlaybackRing *gday_playback_create(uint32_t tracks, uint32_t capacity) {
    if (!tracks || tracks > 32 || !capacity || capacity > 65536) return NULL;
    GdayPlaybackRing *r = calloc(1, sizeof(*r));
    if (!r) return NULL;
    r->samples = calloc((size_t)tracks * 2 * capacity, sizeof(float));
    if (!r->samples) { free(r); return NULL; }
    r->tracks = tracks; r->capacity = capacity;
    atomic_init(&r->written, 0); atomic_init(&r->consumed, 0); atomic_init(&r->underruns, 0);
    atomic_init(&r->audible, UINT32_MAX);
    return r;
}
void gday_playback_destroy(GdayPlaybackRing *r) { if (r) { free(r->samples); free(r); } }
uint32_t gday_playback_available(GdayPlaybackRing *r) {
    uint64_t read = atomic_load_explicit(&r->consumed, memory_order_acquire);
    uint64_t write = atomic_load_explicit(&r->written, memory_order_acquire);
    return (uint32_t)(write - read);
}
uint32_t gday_playback_free(GdayPlaybackRing *r) { return r->capacity - gday_playback_available(r); }
void gday_playback_write_track(GdayPlaybackRing *r, uint32_t track, const float *left, const float *right, uint32_t frames) {
    if (track >= r->tracks || frames > gday_playback_free(r)) return;
    uint64_t write = atomic_load_explicit(&r->written, memory_order_relaxed);
    for (uint32_t i = 0; i < frames; i++) {
        size_t base = ((size_t)track * r->capacity + (write + i) % r->capacity) * 2;
        r->samples[base] = left[i]; r->samples[base + 1] = right[i];
    }
}
void gday_playback_commit(GdayPlaybackRing *r, uint32_t frames) {
    atomic_fetch_add_explicit(&r->written, frames, memory_order_release);
}
void gday_playback_set_audible(GdayPlaybackRing *r, uint32_t mask) { atomic_store_explicit(&r->audible, mask, memory_order_relaxed); }
uint32_t gday_playback_render(GdayPlaybackRing *r, float *left, float *right, uint32_t frames) {
    memset(left, 0, frames * sizeof(float)); memset(right, 0, frames * sizeof(float));
    uint64_t read = atomic_load_explicit(&r->consumed, memory_order_relaxed);
    uint64_t write = atomic_load_explicit(&r->written, memory_order_acquire);
    uint32_t available = (uint32_t)(write - read);
    uint32_t count = frames < available ? frames : available;
    uint32_t mask = atomic_load_explicit(&r->audible, memory_order_relaxed);
    uint32_t active = 0;
    for (uint32_t t = 0; t < r->tracks; t++) if (mask & (1U << t)) active++;
    float gain = active ? 1.0f / active : 0;
    for (uint32_t t = 0; t < r->tracks; t++) {
        if (!(mask & (1U << t))) continue;
        for (uint32_t i = 0; i < count; i++) {
            size_t base = ((size_t)t * r->capacity + (read + i) % r->capacity) * 2;
            left[i] += r->samples[base] * gain; right[i] += r->samples[base + 1] * gain;
        }
    }
    if (count < frames) atomic_fetch_add_explicit(&r->underruns, 1, memory_order_relaxed);
    atomic_store_explicit(&r->consumed, read + count, memory_order_release);
    return count;
}
uint64_t gday_playback_consumed(GdayPlaybackRing *r) { return atomic_load_explicit(&r->consumed, memory_order_acquire); }
uint64_t gday_playback_underruns(GdayPlaybackRing *r) { return atomic_load_explicit(&r->underruns, memory_order_relaxed); }
void gday_playback_reset(GdayPlaybackRing *r) {
    atomic_store(&r->written, 0); atomic_store(&r->consumed, 0); atomic_store(&r->underruns, 0);
}
