#include "AudioCaptureBridge.h"
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>
struct GdayAudioRing {
    uint32_t channels, maxFrames, capacity;
    bool interleaved;
    _Atomic uint64_t writeIndex, readIndex;
    _Atomic int failure;
    float *samples;
    uint32_t *frames;
    uint64_t *times;
};
GdayAudioRing *GdayAudioRingCreate(uint32_t channels, bool interleaved, uint32_t maxFrames, uint32_t capacity) {
    if (!channels || channels > 2 || !maxFrames || maxFrames > 65536 || capacity < 2 || capacity > 1024) return NULL;
    GdayAudioRing *r = calloc(1, sizeof(*r));
    if (!r) return NULL;
    r->channels=channels; r->interleaved=interleaved; r->maxFrames=maxFrames; r->capacity=capacity;
    atomic_init(&r->writeIndex,0); atomic_init(&r->readIndex,0); atomic_init(&r->failure,0);
    if (!atomic_is_lock_free(&r->writeIndex) || !atomic_is_lock_free(&r->readIndex) || !atomic_is_lock_free(&r->failure)) { free(r); return NULL; }
    r->samples=calloc((size_t)channels*maxFrames*capacity,sizeof(float));
    r->frames=calloc(capacity,sizeof(uint32_t)); r->times=calloc(capacity,sizeof(uint64_t));
    if (!r->samples || !r->frames || !r->times) { GdayAudioRingDestroy(r); return NULL; }
    // Touch writable sample and slot-metadata pages on the control thread,
    // avoiding their first-write faults in IOProc. This does not pin the pages.
    volatile float *warm = r->samples;
    size_t sampleCount=(size_t)channels*maxFrames*capacity;
    for(size_t i=0;i<sampleCount;i+=1024) warm[i]=0;
    warm[sampleCount-1]=0;
    volatile uint32_t *warmFrames = r->frames;
    volatile uint64_t *warmTimes = r->times;
    for(uint32_t i=0;i<capacity;i++) { warmFrames[i]=0; warmTimes[i]=0; }
    return r;
}
void GdayAudioRingDestroy(GdayAudioRing *r) { if(r) { free(r->samples); free(r->frames); free(r->times); free(r); } }
static bool fail(GdayAudioRing *r,int code) { int expected=0; atomic_compare_exchange_strong_explicit(&r->failure,&expected,code,memory_order_relaxed,memory_order_relaxed); return false; }
bool GdayAudioRingPush(GdayAudioRing *r,const AudioBufferList *input,const AudioTimeStamp *time) {
    // Hard realtime: fixed bounded copies and lock-free atomics only. Single producer.
    if (!r || atomic_load_explicit(&r->failure,memory_order_relaxed)) return false;
    if (!time || !(time->mFlags & kAudioTimeStampHostTimeValid)) return fail(r,3);
    uint32_t buffers=r->interleaved ? 1 : r->channels;
    if (!input || input->mNumberBuffers != buffers) return fail(r,2);
    uint32_t stride=r->interleaved ? r->channels : 1;
    uint32_t bytes=input->mBuffers[0].mDataByteSize;
    if (bytes % (sizeof(float)*stride)) return fail(r,2);
    uint32_t frames=bytes/(sizeof(float)*stride);
    if (!frames) return true;
    if (frames>r->maxFrames) return fail(r,2);
    for(uint32_t b=0;b<buffers;b++) if(input->mBuffers[b].mNumberChannels!=stride || input->mBuffers[b].mDataByteSize!=bytes) return fail(r,2);
    uint64_t w=atomic_load_explicit(&r->writeIndex,memory_order_relaxed);
    uint64_t rd=atomic_load_explicit(&r->readIndex,memory_order_acquire);
    if(w-rd>=r->capacity) return fail(r,1);
    size_t slot=w%r->capacity;
    float *out=r->samples+slot*r->maxFrames*r->channels;
    if(r->interleaved) { if(input->mBuffers[0].mData) memcpy(out,input->mBuffers[0].mData,bytes); else memset(out,0,bytes); }
    else for(uint32_t c=0;c<r->channels;c++) { const float *in=input->mBuffers[c].mData; for(uint32_t f=0;f<frames;f++) out[f*r->channels+c]=in ? in[f] : 0; }
    r->frames[slot]=frames; r->times[slot]=time->mHostTime;
    atomic_store_explicit(&r->writeIndex,w+1,memory_order_release); return true;
}
bool GdayAudioRingRead(GdayAudioRing *r,float *destination,uint32_t capacityFrames,uint32_t *frames,uint64_t *hostTime) {
    if(!r || !destination || !frames || !hostTime) return false;
    uint64_t rd=atomic_load_explicit(&r->readIndex,memory_order_relaxed);
    if(rd==atomic_load_explicit(&r->writeIndex,memory_order_acquire)) return false;
    size_t slot=rd%r->capacity;
    if(capacityFrames<r->frames[slot]) return fail(r,2);
    *frames=r->frames[slot]; *hostTime=r->times[slot];
    memcpy(destination,r->samples+slot*r->maxFrames*r->channels,(size_t)*frames*r->channels*sizeof(float));
    atomic_store_explicit(&r->readIndex,rd+1,memory_order_release); return true;
}
int GdayAudioRingFailure(GdayAudioRing *r) { return r ? atomic_load_explicit(&r->failure,memory_order_relaxed) : 2; }
OSStatus GdayAudioIOProc(AudioObjectID device,const AudioTimeStamp *now,const AudioBufferList *input,const AudioTimeStamp *inputTime,AudioBufferList *output,const AudioTimeStamp *outputTime,void *context) {
    (void)device; (void)now; (void)output; (void)outputTime;
    GdayAudioRingPush(context,input,inputTime); return noErr;
}
