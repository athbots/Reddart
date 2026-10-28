#ifndef REDDART_H
#define REDDART_H

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct AppleEngine AppleEngine;

AppleEngine* reddart_engine_create(void);
void reddart_engine_destroy(AppleEngine* engine);

void reddart_engine_seek(AppleEngine* engine, int64_t playhead_us);
int64_t reddart_engine_get_playhead(const AppleEngine* engine);
int32_t reddart_engine_split_clip(AppleEngine* engine);
int32_t reddart_engine_delete_selected(AppleEngine* engine);
int32_t reddart_engine_undo(AppleEngine* engine);
int32_t reddart_engine_redo(AppleEngine* engine);

int32_t reddart_engine_apply_lut(AppleEngine* engine, const char* lut_name);
char* reddart_engine_get_state_json(const AppleEngine* engine);
void reddart_string_free(char* ptr);

char* reddart_engine_generate_captions(AppleEngine* engine);
int32_t reddart_engine_denoise_audio(AppleEngine* engine);
char* reddart_engine_auto_reframe(AppleEngine* engine, float target_aspect_w, float target_aspect_h);

int32_t reddart_engine_start_export(AppleEngine* engine, const char* preset, const char* out_path);
float reddart_engine_get_export_progress(const AppleEngine* engine);

int32_t reddart_engine_render_frame_to_rgba(const AppleEngine* engine, uint32_t width, uint32_t height, uint8_t* buffer_out);
int32_t reddart_engine_attach_metal_texture(AppleEngine* engine, void* texture_ptr);

#ifdef __cplusplus
}
#endif

#endif // REDDART_H
