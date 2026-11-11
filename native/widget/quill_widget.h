// Quill's C ABI over widget-sandbox's WidgetHost and TwidgetCore
// (third_party/widgets). One qw_widget per message widget: the QuickJS
// sandbox and the trusted validator (WidgetHost, its own threads), and the
// litehtml renderer, whose fonts, text, images and canvases come from Rust
// through qw_callbacks and whose display lists Rust paints
// (src/ui/widget_runtime/). docs/decisions/message-widgets.md.
//
// Threading: qw_create / qw_destroy on the owner's thread (never a callback);
// every other call from any one thread at a time (Quill's widget thread).
// Callbacks arrive on the WidgetHost thread and on the calling thread; each
// must return without calling back into this API. Strings returned by qw_*
// are freed with qw_string_free. No call throws.
#ifndef QUILL_WIDGET_H
#define QUILL_WIDGET_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct qw_widget qw_widget;

/// twlite::FontMetrics, CSS px.
typedef struct qw_font_metrics {
    float ascent, descent, height, x_height, zero_width, em_ascent, em_descent;
} qw_font_metrics;

typedef struct qw_callbacks {
    void *ctx;
    /// A JSON event for Quill (docs in quill_widget.cpp, `Events`).
    void (*event)(void *ctx, const char *json, size_t len);
    /// twlite::Backend, called with the renderer's lock held.
    int32_t (*create_font)(void *ctx, const char *families, size_t families_len, float size, int32_t weight, bool italic,
                           const char *features, size_t features_len, qw_font_metrics *out);
    void (*delete_font)(void *ctx, int32_t font);
    float (*text_width)(void *ctx, int32_t font, const char *text, size_t len);
    bool (*load_image)(void *ctx, int32_t id, const char *url, size_t len, float *width, float *height);
    /// The canvas ops as twlite::to_json(std::vector<CanvasOp>).
    void (*canvas_run)(void *ctx, int32_t id, int32_t width, int32_t height, const char *ops, size_t len);
    /// Straight RGBA of a region into `rgba` (capacity >= w*h*4); false when it cannot.
    bool (*canvas_read)(void *ctx, int32_t id, int32_t x, int32_t y, int32_t w, int32_t h, uint8_t *rgba, size_t capacity);
    void (*canvas_release)(void *ctx, int32_t id);
} qw_callbacks;

/// The bundles every widget runs (WidgetJS), copied once per process; false when they cannot be used.
bool qw_resources_init(const uint8_t *sandbox_js, size_t sandbox_js_len, const uint8_t *sandbox_qbc, size_t sandbox_qbc_len,
                       const uint8_t *validator_js, size_t validator_js_len, const uint8_t *validator_qbc, size_t validator_qbc_len);

/// `options` is JSON (`Options` in quill_widget.cpp). NULL on failure, with the reason in *error (free it).
qw_widget *qw_create(const char *html, size_t html_len, const char *options, size_t options_len, const qw_callbacks *callbacks,
                     char **error);
void qw_start(qw_widget *widget);
/// Stops the sandbox and the renderer and frees them; no callback comes after it returns.
void qw_destroy(qw_widget *widget);

void qw_set_visible(qw_widget *widget, bool visible);
/// WidgetTheme as JSON: {"colorScheme","params":{…},"button":{…},"text":{…}}.
void qw_set_theme(qw_widget *widget, const char *theme, size_t len);
/// WidgetData as JSON: {"chatData":…,"publicData":…}, null for none.
void qw_set_data(qw_widget *widget, const char *data, size_t len);
void qw_set_width(qw_widget *widget, double width);
void qw_set_max_height(qw_widget *widget, double height);
/// The answer to request `token` (an event "request"): failure 0 answered (result may be NULL), 1 timeout,
/// 2 rate limited, 3 failed.
void qw_respond(qw_widget *widget, uint64_t token, int32_t failure, const char *result, size_t len);

/// Input in CSS px, as twl_pointer / twl_hover / twl_wheel / twl_key.
void qw_pointer(qw_widget *widget, int32_t action, double x, double y, int32_t pointer, bool primary, bool mouse,
                int32_t modifiers, int32_t button);
void qw_hover(qw_widget *widget, int32_t action, double x, double y, int32_t modifiers);
void qw_wheel(qw_widget *widget, double x, double y, double dx, double dy, int32_t mode, int32_t modifiers);
void qw_key(qw_widget *widget, const char *type, const char *key, const char *code, bool repeat, int32_t modifiers);
/// The selected text ("" without one); qw_clear_selection drops it.
char *qw_selected_text(qw_widget *widget);
bool qw_clear_selection(qw_widget *widget);
/// The CSS cursor where the mouse last was ("auto" when the document says nothing).
char *qw_cursor(qw_widget *widget);

typedef struct qw_state {
    /// The document changed since the last qw_paint.
    bool dirty;
    /// Animations run or moves wait: tick on every display frame.
    bool wants_tick;
    /// The widget asked for an animation frame (requestAnimationFrame).
    bool frame_requested;
    /// Seconds until a tick is due anyway when !wants_tick (infinity: none).
    double next_tick_in;
    /// Physical size of the next paint.
    uint32_t px_width, px_height;
} qw_state;
void qw_get_state(qw_widget *widget, qw_state *out);
/// One display frame at `time_ms` (performance.now() of the frame): the widget's frame callbacks, then a tick.
void qw_tick(qw_widget *widget, double time_ms);
/// The display list to paint as JSON: {"w","h","dpr","damage":null|[[x,y,w,h]…],"list":[…]} (twlite::to_json);
/// NULL when there is nothing to paint.
char *qw_paint(qw_widget *widget);
/// Drops caches made again on demand (off screen, memory pressure).
void qw_trim(qw_widget *widget);

void qw_string_free(char *s);

#ifdef __cplusplus
}
#endif
#endif
