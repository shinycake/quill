// Quill's glue between widget-sandbox's WidgetHost, TwidgetCore and Rust
// (quill_widget.h). Quill's own code, MIT; the two libraries it drives are
// BSD-3-Clause (third_party/widgets). docs/decisions/message-widgets.md.
//
// Events (qw_callbacks::event), JSON objects with a "type":
//   kick      the renderer's state changed (qw_get_state): paint, tick or schedule
//   change    {loaded, rendered, dead, error, height, anchor, watch}: the diagnostics Quill shows
//   action    {kind: "openLink" | "openTelegramLink" | "haptic" | "ready", url}
//   storage   {area: "local" | "session", values: {key: value}}
//   gesture   {x, hold}: what the widget claims of a primary press
//   request   {token, data}: answer with qw_respond
//   copy      {text}
//   watch     {on}: poll public_data while visible
//   note      {kind, detail}: for the widget log
//
// Options (qw_create): {width, dpr, maxHeight, visible, locale, policy, theme, storage: {local, session},
//   data: {chatData, publicData}, context: {isCopy, canQuery, backgroundQuery, chatType, viewer: {id, firstName,
//   lastName, username, isPremium}}, safeArea: {top, right, bottom, left, cornerLeft, cornerRight}}.

#include "quill_widget.h"

#include "renderer.h"
#include "twidget/json.h"
#include "twidget/widget_host.h"
#ifdef _WIN32
#include "twidget/icu_intl.h"
#endif

#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

using twidget::Json;

namespace {

std::mutex g_resources_mutex;
std::shared_ptr<twidget::SandboxResources> g_resources;

char *dup_string(const std::string &s) {
    char *out = static_cast<char *>(std::malloc(s.size() + 1));
    if (!out) return nullptr;
    std::memcpy(out, s.data(), s.size());
    out[s.size()] = '\0';
    return out;
}

std::string str(const char *p, size_t len) { return p && len ? std::string(p, len) : std::string(); }

double now_ms() {
    return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now().time_since_epoch()).count();
}

std::optional<std::string> opt_string(const Json &v) {
    if (v.isString()) return v.asString();
    return std::nullopt;
}

/// Fonts, text, images and canvases from Rust.
class RustBackend final : public twlite::Backend {
public:
    explicit RustBackend(const qw_callbacks &cb) : cb_(cb) {}

    int create_font(const std::string &families, float size_px, int weight, bool italic, const std::string &features,
                    twlite::FontMetrics &out) override {
        if (!cb_.create_font) return 0;
        qw_font_metrics m{};
        int font = cb_.create_font(cb_.ctx, families.data(), families.size(), size_px, weight, italic, features.data(),
                                   features.size(), &m);
        out.ascent = m.ascent;
        out.descent = m.descent;
        out.height = m.height;
        out.x_height = m.x_height;
        out.zero_width = m.zero_width;
        out.em_ascent = m.em_ascent;
        out.em_descent = m.em_descent;
        return font;
    }
    void delete_font(int font) override {
        if (cb_.delete_font) cb_.delete_font(cb_.ctx, font);
    }
    float text_width(int font, const std::string &text) override {
        return cb_.text_width ? cb_.text_width(cb_.ctx, font, text.data(), text.size()) : 0.0f;
    }
    bool load_image(int id, const std::string &url, float &w, float &h) override {
        return cb_.load_image && cb_.load_image(cb_.ctx, id, url.data(), url.size(), &w, &h);
    }
    void canvas_run(int id, int w, int h, const std::vector<twlite::CanvasOp> &ops) override {
        if (!cb_.canvas_run) return;
        std::string json = twlite::to_json(ops);
        cb_.canvas_run(cb_.ctx, id, w, h, json.data(), json.size());
    }
    bool canvas_read(int id, int x, int y, int w, int h, std::vector<uint8_t> &rgba) override {
        if (!cb_.canvas_read || w <= 0 || h <= 0 || w > 16384 || h > 16384) return false;
        rgba.assign(size_t(w) * size_t(h) * 4, 0);
        return cb_.canvas_read(cb_.ctx, id, x, y, w, h, rgba.data(), rgba.size());
    }
    void canvas_release(int id) override {
        if (cb_.canvas_release) cb_.canvas_release(cb_.ctx, id);
    }

private:
    qw_callbacks cb_;
};

/// WidgetHost's renderer on TwidgetCore. WidgetHost calls the Renderer members on its host thread; Quill's
/// widget thread calls the rest (input, ticks, paints). Every call into the core takes `mutex_`; messages
/// to the host go out after it is released, and Rust hears of each change through a `kick`.
class QuillRenderer final : public twidget::Renderer {
public:
    QuillRenderer(const qw_callbacks &cb, bool dark, double width, double dpr, double max_height)
        : cb_(cb), backend_(cb), core_(std::make_unique<twlite::Renderer>(width, dpr, dark, backend_)) {
        std::vector<Json> out;
        core_->set_max_height(max_height, out);
    }
    ~QuillRenderer() override { destroy(); }

    void load() override {
        if (onMessage) onMessage("{\"type\":\"ready\"}");
        kick();
    }
    void send(const std::string &json) override {
        if (json.rfind("{\"type\":\"requestFrame\"", 0) == 0) {
            frame_requested_ = true;
            kick();
            return;
        }
        auto msg = Json::parse(json);
        if (!msg) {
            if (onDiag) onDiag("renderer-violation", "bad message");
            return;
        }
        run([&](std::vector<Json> &out) { out = core_->receive(*msg); });
    }
    void setTheme(const std::string &theme) override {
        run([&](std::vector<Json> &out) { core_->set_theme(theme == "dark", out); });
    }
    void setWidth(double width) override {
        run([&](std::vector<Json> &out) { core_->set_width(width, out); });
    }
    void destroy() override {
        std::lock_guard<std::mutex> l(mutex_);
        destroyed_ = true;
        core_.reset();
    }

    void set_max_height(double height) {
        run([&](std::vector<Json> &out) { core_->set_max_height(height, out); });
    }
    void pointer(int action, double x, double y, int pointer, bool primary, bool mouse, int modifiers, int button) {
        run([&](std::vector<Json> &out) { out = core_->touch(action, x, y, pointer, primary, mouse, modifiers, button); });
    }
    void hover(int action, double x, double y, int modifiers) {
        run([&](std::vector<Json> &out) { out = core_->hover(action, x, y, modifiers); });
    }
    void wheel(double x, double y, double dx, double dy, int mode, int modifiers) {
        run([&](std::vector<Json> &out) { out = core_->wheel(x, y, dx, dy, mode, modifiers); });
    }
    void key(const std::string &type, const std::string &key, const std::string &code, bool repeat, int modifiers) {
        run([&](std::vector<Json> &out) { out = core_->key(type, key, code, repeat, modifiers); });
    }
    std::string selected_text() {
        std::lock_guard<std::mutex> l(mutex_);
        return destroyed_ ? std::string() : core_->selected_text();
    }
    bool clear_selection() {
        bool cleared = false;
        run([&](std::vector<Json> &) { cleared = core_->clear_selection(); });
        return cleared;
    }
    std::string cursor() {
        std::lock_guard<std::mutex> l(mutex_);
        return destroyed_ ? std::string("auto") : core_->cursor();
    }
    void trim() {
        std::lock_guard<std::mutex> l(mutex_);
        if (!destroyed_) core_->trim();
    }
    void state(qw_state &s) {
        std::lock_guard<std::mutex> l(mutex_);
        s.frame_requested = frame_requested_.load();
        if (destroyed_) {
            s.dirty = s.wants_tick = false;
            s.next_tick_in = INFINITY;
            s.px_width = s.px_height = 0;
            return;
        }
        s.dirty = core_->dirty();
        s.wants_tick = core_->wants_tick();
        s.next_tick_in = core_->next_tick_in();
        s.px_width = core_->px_width();
        s.px_height = core_->px_height();
    }
    void tick(double time_ms) {
        if (frame_requested_.exchange(false) && onMessage) {
            onMessage("{\"type\":\"frame\",\"time\":" + Json::number(time_ms) + "}");
        }
        run([&](std::vector<Json> &out) { out = core_->tick(); });
    }
    std::string paint() {
        std::lock_guard<std::mutex> l(mutex_);
        if (destroyed_) return {};
        uint32_t w = core_->px_width(), h = core_->px_height();
        if (w == 0 || h == 0) return {};
        twlite::DisplayList list = core_->paint();
        std::string out = "{\"w\":" + std::to_string(w) + ",\"h\":" + std::to_string(h) + ",\"dpr\":" +
                          Json::number(core_->dpr()) + ",\"damage\":" + core_->damage_json() + ",\"list\":";
        out += twlite::to_json(list);
        out += "}";
        return out;
    }

private:
    template <class Call> void run(Call call) {
        std::vector<Json> out;
        {
            std::lock_guard<std::mutex> l(mutex_);
            if (destroyed_) return;
            try {
                call(out);
            } catch (const std::exception &e) {
                if (onDiag) onDiag("renderer", e.what());
            } catch (...) {
                if (onDiag) onDiag("renderer", "unknown failure");
            }
        }
        if (onMessage) {
            for (const Json &msg : out) {
                if (msg["type"].asString() == "violation" && onDiag) onDiag("renderer-violation", msg["detail"].asString());
                onMessage(msg.dump());
            }
        }
        kick();
    }
    void kick() {
        if (cb_.event) cb_.event(cb_.ctx, "{\"type\":\"kick\"}", 15);
    }

    qw_callbacks cb_;
    RustBackend backend_;
    std::mutex mutex_;
    std::unique_ptr<twlite::Renderer> core_;
    bool destroyed_ = false;
    std::atomic<bool> frame_requested_{false};
};

twidget::WidgetTheme parse_theme(const Json &j) {
    twidget::WidgetTheme theme;
    theme.colorScheme = j["colorScheme"].asString() == "dark" ? "dark" : "light";
    for (const auto &[key, value] : j["params"].obj()) {
        if (value.isString()) theme.params[key] = value.asString();
    }
    const Json &b = j["button"];
    theme.button.bg = opt_string(b["bg"]);
    theme.button.text = opt_string(b["text"]);
    theme.button.radius = opt_string(b["radius"]);
    theme.button.font = opt_string(b["font"]);
    theme.button.padding = opt_string(b["padding"]);
    theme.button.height = opt_string(b["height"]);
    if (b["pressedOpacity"].isNumber()) theme.button.pressedOpacity = b["pressedOpacity"].asNumber();
    if (b["disabledOpacity"].isNumber()) theme.button.disabledOpacity = b["disabledOpacity"].asNumber();
    const Json &t = j["text"];
    if (t["fontSize"].isNumber()) theme.text.fontSize = t["fontSize"].asNumber();
    if (t["lineHeight"].isNumber()) theme.text.lineHeight = t["lineHeight"].asNumber();
    theme.text.padding = opt_string(t["padding"]);
    return theme;
}

twidget::WidgetData parse_data(const Json &j) { return twidget::WidgetData{opt_string(j["chatData"]), opt_string(j["publicData"])}; }

twidget::StorageArea parse_area(const Json &j) {
    twidget::StorageArea area;
    for (const auto &[key, value] : j.obj()) {
        if (value.isString()) area[key] = value.asString();
    }
    return area;
}

Json area_json(const twidget::StorageArea &area) {
    Json out = Json::object();
    for (const auto &[key, value] : area) out.set(key, value);
    return out;
}

} // namespace

struct qw_widget {
    qw_callbacks cb{};
    QuillRenderer *renderer = nullptr;
    std::unique_ptr<twidget::WidgetHost> host;
    std::mutex requests_mutex;
    uint64_t next_token = 1;
    std::map<uint64_t, std::function<void(const twidget::WidgetRequestOutcome &)>> requests;
    std::string last_change;

    void emit(const Json &event) {
        if (!cb.event) return;
        std::string json = event.dump();
        cb.event(cb.ctx, json.data(), json.size());
    }
};

namespace {

void wire(qw_widget *w) {
    twidget::WidgetHost &host = *w->host;
    host.onChange = [w](const twidget::WidgetDiagnostics &d) {
        Json e = Json::object();
        e.set("type", "change");
        e.set("loaded", d.loaded);
        e.set("rendered", d.rendered);
        e.set("dead", d.dead ? Json(*d.dead) : Json());
        e.set("error", d.lastError ? Json(*d.lastError) : Json());
        e.set("height", d.height);
        e.set("anchor", d.resizeAnchor);
        e.set("watch", d.watchesPublicData);
        std::string json = e.dump();
        if (json == w->last_change) return;
        w->last_change = json;
        if (w->cb.event) w->cb.event(w->cb.ctx, json.data(), json.size());
    };
    host.onAction = [w](const twidget::WidgetAction &action) {
        Json e = action.toJson();
        Json out = Json::object();
        out.set("type", "action");
        out.set("kind", e["kind"].asString());
        out.set("url", action.url);
        w->emit(out);
    };
    host.onStorageCommit = [w](const std::string &area, const std::map<std::string, std::string> &values) {
        Json e = Json::object();
        e.set("type", "storage");
        e.set("area", area);
        e.set("values", area_json(values));
        w->emit(e);
    };
    host.onGesture = [w](const twidget::WidgetGestureClaim &claim) {
        Json e = Json::object();
        e.set("type", "gesture");
        e.set("x", claim.x);
        e.set("hold", claim.hold);
        w->emit(e);
    };
    host.onRequest = [w](const twidget::WidgetRequest &request, std::function<void(const twidget::WidgetRequestOutcome &)> respond) {
        // The context never offers a wallet; a request that still asks for the
        // wallet card fails here and never reaches TDLib.
        if (request.wallet) {
            twidget::WidgetRequestOutcome refused;
            refused.failure = twidget::WidgetRequestFailure::Failed;
            respond(refused);
            return;
        }
        uint64_t token;
        {
            std::lock_guard<std::mutex> l(w->requests_mutex);
            token = w->next_token++;
            w->requests[token] = std::move(respond);
        }
        Json e = Json::object();
        e.set("type", "request");
        e.set("token", double(token));
        e.set("data", request.data);
        w->emit(e);
    };
    host.onCopyText = [w](const std::string &text) {
        Json e = Json::object();
        e.set("type", "copy");
        e.set("text", text);
        w->emit(e);
    };
    host.onWatchPublicData = [w](bool on) {
        Json e = Json::object();
        e.set("type", "watch");
        e.set("on", on);
        w->emit(e);
    };
    host.onError = [w](const std::string &text) {
        Json e = Json::object();
        e.set("type", "note");
        e.set("kind", "error");
        e.set("detail", text);
        w->emit(e);
    };
}

} // namespace

extern "C" {

bool qw_resources_init(const uint8_t *sandbox_js, size_t sandbox_js_len, const uint8_t *sandbox_qbc, size_t sandbox_qbc_len,
                       const uint8_t *validator_js, size_t validator_js_len, const uint8_t *validator_qbc, size_t validator_qbc_len) {
    try {
        auto res = std::make_shared<twidget::SandboxResources>();
        res->sandboxBundle.assign(reinterpret_cast<const char *>(sandbox_js), sandbox_js_len);
        res->validatorBundle.assign(reinterpret_cast<const char *>(validator_js), validator_js_len);
        res->sandboxBytecode.assign(sandbox_qbc, sandbox_qbc + sandbox_qbc_len);
        res->validatorBytecode.assign(validator_qbc, validator_qbc + validator_qbc_len);
        std::lock_guard<std::mutex> l(g_resources_mutex);
        g_resources = std::move(res);
        return true;
    } catch (...) {
        return false;
    }
}

qw_widget *qw_create(const char *html, size_t html_len, const char *options, size_t options_len, const qw_callbacks *callbacks,
                     char **error) {
    if (error) *error = nullptr;
    try {
        std::shared_ptr<twidget::SandboxResources> resources;
        {
            std::lock_guard<std::mutex> l(g_resources_mutex);
            resources = g_resources;
        }
        if (!resources) throw std::runtime_error("the widget bundles are not loaded");
        if (!callbacks) throw std::runtime_error("no callbacks");
        auto parsed = Json::parse(str(options, options_len));
        if (!parsed) throw std::runtime_error("bad options");
        const Json &o = *parsed;
        double width = o["width"].asNumber(320);
        double dpr = o["dpr"].asNumber(1);
        double max_height = o["maxHeight"].asNumber(600);

        twidget::WidgetHostOptions opts;
        opts.visible = o["visible"].asBool(true);
        opts.devicePixelRatio = dpr;
        if (o["locale"].isString()) opts.locale = o["locale"].asString();
        if (o["policy"].isString()) opts.policy = o["policy"].asString();
        opts.storage.local = parse_area(o["storage"]["local"]);
        opts.storage.session = parse_area(o["storage"]["session"]);
        opts.data = parse_data(o["data"]);
        const Json &c = o["context"];
        opts.context.isCopy = c["isCopy"].asBool();
        opts.context.canQuery = c["canQuery"].asBool();
        opts.context.backgroundQuery = c["backgroundQuery"].asBool();
        opts.context.chatType = c["chatType"].asString();
        // Quill never offers a wallet (TON is out of scope): no `TelegramWidget.wallet`, no wallet card.
        opts.context.wallet = false;
        if (c["viewer"].isObject()) {
            const Json &v = c["viewer"];
            twidget::WidgetViewer viewer;
            viewer.id = int64_t(v["id"].asNumber());
            viewer.firstName = opt_string(v["firstName"]);
            viewer.lastName = opt_string(v["lastName"]);
            viewer.username = opt_string(v["username"]);
            viewer.isPremium = v["isPremium"].asBool();
            opts.context.viewer = viewer;
        }
        twidget::WidgetTheme theme = parse_theme(o["theme"]);

        auto w = std::make_unique<qw_widget>();
        w->cb = *callbacks;
        qw_widget *raw = w.get();
        auto factory = [raw, dpr, max_height](std::shared_ptr<twidget::SandboxResources>, const std::string &scheme,
                                              double width) -> std::unique_ptr<twidget::Renderer> {
            auto r = std::make_unique<QuillRenderer>(raw->cb, scheme == "dark", width, dpr, max_height);
            raw->renderer = r.get();
            return r;
        };
        std::shared_ptr<twidget::IntlProvider> intl;
#ifdef _WIN32
        intl = std::make_shared<twidget::IcuIntl>(opts.locale);
#else
        intl = std::make_shared<twidget::BasicIntl>();
#endif
        w->host = std::make_unique<twidget::WidgetHost>(str(html, html_len), std::move(theme), width, std::move(opts),
                                                         std::move(resources), factory, std::move(intl));
        // Quill draws "Widget stopped" itself (centred, as Telegram Desktop does); the renderer's own badge stays empty.
        w->host->stoppedLabel.clear();
        // The host thread already runs (the validator's and renderer's
        // diagnostics post to it) and reads these callbacks there: set them
        // on that thread, never next to a read.
        w->host->onHostThread([raw] { wire(raw); });
        return w.release();
    } catch (const std::exception &e) {
        if (error) *error = dup_string(e.what());
    } catch (...) {
        if (error) *error = dup_string("unknown failure");
    }
    return nullptr;
}

void qw_start(qw_widget *w) {
    if (!w) return;
    try {
        w->host->start();
    } catch (...) {
    }
}

void qw_destroy(qw_widget *w) {
    if (!w) return;
    try {
        w->host->destroy();
        std::map<uint64_t, std::function<void(const twidget::WidgetRequestOutcome &)>> pending;
        {
            std::lock_guard<std::mutex> l(w->requests_mutex);
            pending.swap(w->requests);
        }
        w->host.reset();
    } catch (...) {
    }
    delete w;
}

void qw_set_visible(qw_widget *w, bool visible) {
    if (!w) return;
    try {
        w->host->setVisible(visible);
    } catch (...) {
    }
}

void qw_set_theme(qw_widget *w, const char *theme, size_t len) {
    if (!w) return;
    try {
        if (auto j = Json::parse(str(theme, len))) w->host->setTheme(parse_theme(*j));
    } catch (...) {
    }
}

void qw_set_data(qw_widget *w, const char *data, size_t len) {
    if (!w) return;
    try {
        if (auto j = Json::parse(str(data, len))) w->host->setData(parse_data(*j));
    } catch (...) {
    }
}


void qw_set_width(qw_widget *w, double width) {
    if (!w || !w->renderer) return;
    try {
        w->renderer->setWidth(width);
    } catch (...) {
    }
}

void qw_set_max_height(qw_widget *w, double height) {
    if (!w || !w->renderer) return;
    try {
        w->renderer->set_max_height(height);
    } catch (...) {
    }
}

void qw_respond(qw_widget *w, uint64_t token, int32_t failure, const char *result, size_t len) {
    if (!w) return;
    try {
        std::function<void(const twidget::WidgetRequestOutcome &)> respond;
        {
            std::lock_guard<std::mutex> l(w->requests_mutex);
            auto it = w->requests.find(token);
            if (it == w->requests.end()) return;
            respond = std::move(it->second);
            w->requests.erase(it);
        }
        twidget::WidgetRequestOutcome outcome;
        switch (failure) {
        case 0:
            outcome.failure = twidget::WidgetRequestFailure::None;
            if (result) outcome.answer.result = str(result, len);
            break;
        case 1: outcome.failure = twidget::WidgetRequestFailure::Timeout; break;
        case 2: outcome.failure = twidget::WidgetRequestFailure::RateLimited; break;
        default: outcome.failure = twidget::WidgetRequestFailure::Failed; break;
        }
        if (respond) respond(outcome);
    } catch (...) {
    }
}

void qw_pointer(qw_widget *w, int32_t action, double x, double y, int32_t pointer, bool primary, bool mouse, int32_t modifiers,
                int32_t button) {
    if (!w || !w->renderer) return;
    try {
        w->renderer->pointer(action, x, y, pointer, primary, mouse, modifiers, button);
    } catch (...) {
    }
}

void qw_hover(qw_widget *w, int32_t action, double x, double y, int32_t modifiers) {
    if (!w || !w->renderer) return;
    try {
        w->renderer->hover(action, x, y, modifiers);
    } catch (...) {
    }
}

void qw_wheel(qw_widget *w, double x, double y, double dx, double dy, int32_t mode, int32_t modifiers) {
    if (!w || !w->renderer) return;
    try {
        w->renderer->wheel(x, y, dx, dy, mode, modifiers);
    } catch (...) {
    }
}

void qw_key(qw_widget *w, const char *type, const char *key, const char *code, bool repeat, int32_t modifiers) {
    if (!w || !w->renderer || !type || !key || !code) return;
    try {
        w->renderer->key(type, key, code, repeat, modifiers);
    } catch (...) {
    }
}

char *qw_selected_text(qw_widget *w) {
    if (!w || !w->renderer) return dup_string("");
    try {
        return dup_string(w->renderer->selected_text());
    } catch (...) {
        return dup_string("");
    }
}

bool qw_clear_selection(qw_widget *w) {
    if (!w || !w->renderer) return false;
    try {
        return w->renderer->clear_selection();
    } catch (...) {
        return false;
    }
}

char *qw_cursor(qw_widget *w) {
    if (!w || !w->renderer) return dup_string("auto");
    try {
        return dup_string(w->renderer->cursor());
    } catch (...) {
        return dup_string("auto");
    }
}

void qw_get_state(qw_widget *w, qw_state *out) {
    if (!out) return;
    *out = qw_state{false, false, false, INFINITY, 0, 0};
    if (!w || !w->renderer) return;
    try {
        w->renderer->state(*out);
    } catch (...) {
    }
}

void qw_tick(qw_widget *w, double time_ms) {
    if (!w || !w->renderer) return;
    try {
        w->renderer->tick(time_ms > 0 ? time_ms : now_ms());
    } catch (...) {
    }
}

char *qw_paint(qw_widget *w) {
    if (!w || !w->renderer) return nullptr;
    try {
        std::string json = w->renderer->paint();
        return json.empty() ? nullptr : dup_string(json);
    } catch (...) {
        return nullptr;
    }
}

void qw_trim(qw_widget *w) {
    if (!w || !w->renderer) return;
    try {
        w->renderer->trim();
    } catch (...) {
    }
}

void qw_string_free(char *s) { std::free(s); }

} // extern "C"
