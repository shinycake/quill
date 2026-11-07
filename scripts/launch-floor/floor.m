// The cold launch floor: a macOS app with no app code that opens one window
// the way Quill does (docs/decisions/launch-cold-floor.md). It orders the
// window front, flushes the Core Animation transaction and only then runs
// NSApp, with an empty main menu installed first so finishLaunching does not
// build AppKit's default one.
//
// Environment (scripts/launch-floor/floorbench.py sets these):
//   FLOOR_TRACE=path     append "<mark> <unix µs>" lines: exec (the process
//                        start, as Quill's own trace has it), main, ordered,
//                        flushed, will_finish, did_finish
//   FLOOR_BORDERLESS=1   a borderless window (no title bar buttons)
//   FLOOR_NO_FLUSH=1     skip the flush, as a plain AppKit app does
//   FLOOR_PRELOAD=a:b    dlopen each path in turn on a background thread from
//                        main, then mark preloaded
//   FLOOR_STAGED=a:b     the same, one image per dlopen, dependencies first
//                        (as gpui-pre-macos/src/staged_load.rs), then mark
//                        preloaded
//   FLOOR_THEN_TITLED=1  a borderless window, made titled after the first
//                        run loop pass; marks titled
//   FLOOR_DEFER_WT=1     hold AppKit's Siri availability observer (the first
//                        use of NSTextInputContext loads Writing Tools for it)
//                        until a worker started from applicationDidFinishLaunching:
//                        has loaded WritingToolsUI; marks wt_loaded, wt_ready
// The idle mark is the first time the main run loop waits for events.
#import <AppKit/AppKit.h>
#import <Metal/Metal.h>
#import <QuartzCore/QuartzCore.h>
#include <dlfcn.h>
#include <libproc.h>
#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <objc/runtime.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>

static FILE *trace;

static void mark(const char *name) {
  if (!trace) return;
  struct timeval now;
  gettimeofday(&now, NULL);
  fprintf(trace, "%s %lld\n", name, (long long)now.tv_sec * 1000000 + now.tv_usec);
  fflush(trace);
}

static void mark_exec(void) {
  struct proc_bsdinfo info;
  if (!trace || proc_pidinfo(getpid(), PROC_PIDTBSDINFO, 0, &info, sizeof info) != sizeof info) return;
  fprintf(trace, "exec %lld\n", (long long)info.pbi_start_tvsec * 1000000 + info.pbi_start_tvusec);
  fflush(trace);
}

static void *preload(void *paths) {
  char *rest = strdup(paths);
  for (char *path = strsep(&rest, ":"); path; path = strsep(&rest, ":")) {
    if (*path) dlopen(path, RTLD_LAZY | RTLD_LOCAL);
  }
  mark("preloaded");
  return NULL;
}


struct text_info {
  uint64_t version, load_address_unslid, text_segment_size;
  uuid_t uuid;
  const char *path;
  uint64_t text_segment_offset;
};
extern int dyld_shared_cache_iterate_text(const uuid_t, void (^)(const struct text_info *));
extern bool _dyld_get_shared_cache_uuid(uuid_t);
extern const void *_dyld_get_shared_cache_range(size_t *);

static NSDictionary<NSString *, NSArray *> *staged_graph;
static NSMutableArray<NSString *> *staged_order;
static NSMutableSet<NSString *> *staged_done;

static void staged_visit(NSString *path) {
  if ([staged_done containsObject:path]) return;
  [staged_done addObject:path];
  for (NSString *dep in staged_graph[path]) staged_visit(dep);
  [staged_order addObject:path];
}

static void staged_load(NSString *root) {
  uuid_t uuid;
  size_t len;
  _dyld_get_shared_cache_uuid(uuid);
  const uint8_t *base = _dyld_get_shared_cache_range(&len);
  NSMutableDictionary<NSString *, NSValue *> *headers = [NSMutableDictionary dictionary];
  dyld_shared_cache_iterate_text(uuid, ^(const struct text_info *info) {
    headers[@(info->path)] = [NSValue valueWithPointer:base + info->text_segment_offset];
  });
  NSMutableSet *loaded = [NSMutableSet set];
  for (uint32_t i = 0; i < _dyld_image_count(); i++) [loaded addObject:@(_dyld_get_image_name(i))];
  NSMutableDictionary *graph = [NSMutableDictionary dictionary];
  NSMutableSet *queued = [NSMutableSet setWithObject:root];
  NSArray *level = @[ root ];
  while (level.count) {
    dispatch_apply(level.count, dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^(size_t i) {
      const volatile struct mach_header_64 *mh = [headers[level[i]] pointerValue];
      if (mh) (void)((const volatile uint8_t *)(mh + 1))[mh->sizeofcmds ? mh->sizeofcmds - 1 : 0];
    });
    NSMutableArray *next = [NSMutableArray array];
    for (NSString *path in level) {
      const struct mach_header_64 *mh = [headers[path] pointerValue];
      NSMutableArray *deps = [NSMutableArray array];
      const uint8_t *cmd = mh ? (const uint8_t *)(mh + 1) : NULL;
      for (uint32_t i = 0; mh && i < mh->ncmds; i++) {
        const struct load_command *lc = (const void *)cmd;
        if (lc->cmd == LC_LOAD_DYLIB || lc->cmd == LC_LOAD_WEAK_DYLIB || lc->cmd == LC_REEXPORT_DYLIB) {
          NSString *dep = @((const char *)lc + ((const struct dylib_command *)lc)->dylib.name.offset);
          if (headers[dep] && ![loaded containsObject:dep]) {
            [deps addObject:dep];
            if (![queued containsObject:dep]) { [queued addObject:dep]; [next addObject:dep]; }
          }
        }
        cmd += lc->cmdsize;
      }
      graph[path] = deps;
    }
    level = next;
  }
  staged_graph = graph;
  staged_order = [NSMutableArray array];
  staged_done = [NSMutableSet set];
  staged_visit(root);
  for (NSString *path in staged_order) dlopen(path.UTF8String, RTLD_LAZY);
}

static void *preload_staged(void *paths) {
  @autoreleasepool {
    for (NSString *path in [@((const char *)paths) componentsSeparatedByString:@":"])
      if (path.length) staged_load(path);
  }
  mark("preloaded");
  return NULL;
}

static const char *const WRITING_TOOLS_UI =
    "/System/Library/PrivateFrameworks/WritingToolsUI.framework/WritingToolsUI";
static const char *const ADD_SIRI_OBSERVER =
    "addObserverForEnhancedSiriAvailabilityDidChangeUsingBlock:";
static IMP original_add_siri_observer;
static NSMutableArray *held_siri_observers;

static id hold_siri_observer(id self, SEL _cmd, id block) {
  if (!held_siri_observers) held_siri_observers = [NSMutableArray new];
  [held_siri_observers addObject:[block copy]];
  mark("siri_held");
  return nil;
}

static void defer_writing_tools(void) {
  Class coordinator = objc_getClass("NSWritingToolsCoordinator");
  Method method = coordinator ? class_getClassMethod(coordinator, sel_registerName(ADD_SIRI_OBSERVER)) : NULL;
  if (method) original_add_siri_observer = method_setImplementation(method, (IMP)hold_siri_observer);
}

static void load_writing_tools(void) {
  if (!original_add_siri_observer) return;
  dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
    dlopen(WRITING_TOOLS_UI, RTLD_LAZY);
    mark("wt_loaded");
    dispatch_async(dispatch_get_main_queue(), ^{
      Class coordinator = objc_getClass("NSWritingToolsCoordinator");
      SEL sel = sel_registerName(ADD_SIRI_OBSERVER);
      method_setImplementation(class_getClassMethod(coordinator, sel), original_add_siri_observer);
      for (id block in held_siri_observers) {
        ((id(*)(id, SEL, id))original_add_siri_observer)(coordinator, sel, block);
      }
      held_siri_observers = nil;
      mark("wt_ready");
    });
  });
}

@interface V : NSView
@property(strong) id<MTLDevice> device;
@property(strong) id<MTLCommandQueue> queue;
@end

@implementation V
- (BOOL)wantsUpdateLayer {
  return YES;
}
- (CALayer *)makeBackingLayer {
  CAMetalLayer *layer = [CAMetalLayer layer];
  self.device = MTLCreateSystemDefaultDevice();
  self.queue = [self.device newCommandQueue];
  layer.device = self.device;
  layer.pixelFormat = MTLPixelFormatBGRA8Unorm;
  return layer;
}
- (void)updateLayer {
  CAMetalLayer *layer = (CAMetalLayer *)self.layer;
  layer.drawableSize = CGSizeMake(self.bounds.size.width * 2, self.bounds.size.height * 2);
  id<CAMetalDrawable> drawable = [layer nextDrawable];
  if (!drawable) return;
  MTLRenderPassDescriptor *pass = [MTLRenderPassDescriptor renderPassDescriptor];
  pass.colorAttachments[0].texture = drawable.texture;
  pass.colorAttachments[0].loadAction = MTLLoadActionClear;
  pass.colorAttachments[0].clearColor = MTLClearColorMake(0.2, 0.4, 0.8, 1);
  pass.colorAttachments[0].storeAction = MTLStoreActionStore;
  id<MTLCommandBuffer> commands = [self.queue commandBuffer];
  [[commands renderCommandEncoderWithDescriptor:pass] endEncoding];
  [commands presentDrawable:drawable];
  [commands commit];
}
@end

@interface D : NSObject <NSApplicationDelegate>
@end

@implementation D
- (void)applicationWillFinishLaunching:(NSNotification *)note {
  mark("will_finish");
}
- (void)applicationDidFinishLaunching:(NSNotification *)note {
  mark("did_finish");
  load_writing_tools();
}
@end

int main(void) {
  @autoreleasepool {
    const char *path = getenv("FLOOR_TRACE");
    if (path) trace = fopen(path, "a");
    mark("main");
    mark_exec();
    const char *preload_path = getenv("FLOOR_PRELOAD");
    if (preload_path) {
      pthread_t thread;
      pthread_create(&thread, NULL, preload, (void *)preload_path);
      pthread_detach(thread);
    }
    const char *staged_paths = getenv("FLOOR_STAGED");
    if (staged_paths) {
      pthread_t thread;
      pthread_create(&thread, NULL, preload_staged, (void *)staged_paths);
      pthread_detach(thread);
    }
    CFRunLoopObserverRef idle = CFRunLoopObserverCreateWithHandler(
        NULL, kCFRunLoopBeforeWaiting, false, 0, ^(CFRunLoopObserverRef o, CFRunLoopActivity a) {
          mark("idle");
          CFRunLoopObserverInvalidate(o);
        });
    CFRunLoopAddObserver(CFRunLoopGetMain(), idle, kCFRunLoopCommonModes);
    if (getenv("FLOOR_DEFER_WT")) defer_writing_tools();
    NSApplication *app = [NSApplication sharedApplication];
    D *delegate = [D new];
    app.delegate = delegate;
    [app setActivationPolicy:NSApplicationActivationPolicyRegular];
    app.mainMenu = [NSMenu new];
    NSWindowStyleMask titled = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                               NSWindowStyleMaskResizable | NSWindowStyleMaskMiniaturizable;
    NSWindowStyleMask style =
        getenv("FLOOR_BORDERLESS") || getenv("FLOOR_THEN_TITLED") ? NSWindowStyleMaskBorderless : titled;
    NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(200, 200, 1000, 700)
                                                   styleMask:style
                                                     backing:NSBackingStoreBuffered
                                                       defer:NO];
    V *view = [[V alloc] initWithFrame:NSMakeRect(0, 0, 1000, 700)];
    view.wantsLayer = YES;
    window.contentView = view;
    window.title = @"Floor";
    [window makeKeyAndOrderFront:nil];
    mark("ordered");
    if (!getenv("FLOOR_NO_FLUSH")) {
      [CATransaction flush];
      mark("flushed");
    }
    if (getenv("FLOOR_THEN_TITLED")) {
      dispatch_async(dispatch_get_main_queue(), ^{
        window.styleMask = titled;
        [window makeKeyAndOrderFront:nil];
        [CATransaction flush];
        mark("titled");
      });
    }
    [app run];
  }
  return 0;
}
