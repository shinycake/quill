// The cold launch floor: a macOS app with no app code that opens one window
// the way Quill does (docs/decisions/launch-cold-floor.md). It orders the
// window front, flushes the Core Animation transaction and only then runs
// NSApp, with an empty main menu installed first so finishLaunching does not
// build AppKit's default one.
//
// Environment (scripts/launch-floor/floorbench.py sets these):
//   FLOOR_TRACE=path     append "<mark> <unix µs>" lines: main, ordered,
//                        flushed, will_finish, did_finish
//   FLOOR_BORDERLESS=1   a borderless window (no title bar buttons)
//   FLOOR_NO_FLUSH=1     skip the flush, as a plain AppKit app does
//   FLOOR_PRELOAD=a:b    dlopen each path in turn on a background thread from
//                        main, then mark preloaded
// The idle mark is the first time the main run loop waits for events.
#import <AppKit/AppKit.h>
#import <Metal/Metal.h>
#import <QuartzCore/QuartzCore.h>
#include <dlfcn.h>
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

static void *preload(void *paths) {
  char *rest = strdup(paths);
  for (char *path = strsep(&rest, ":"); path; path = strsep(&rest, ":")) {
    if (*path) dlopen(path, RTLD_LAZY | RTLD_LOCAL);
  }
  mark("preloaded");
  return NULL;
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
}
@end

int main(void) {
  @autoreleasepool {
    const char *path = getenv("FLOOR_TRACE");
    if (path) trace = fopen(path, "a");
    mark("main");
    const char *preload_path = getenv("FLOOR_PRELOAD");
    if (preload_path) {
      pthread_t thread;
      pthread_create(&thread, NULL, preload, (void *)preload_path);
      pthread_detach(thread);
    }
    CFRunLoopObserverRef idle = CFRunLoopObserverCreateWithHandler(
        NULL, kCFRunLoopBeforeWaiting, false, 0, ^(CFRunLoopObserverRef o, CFRunLoopActivity a) {
          mark("idle");
          CFRunLoopObserverInvalidate(o);
        });
    CFRunLoopAddObserver(CFRunLoopGetMain(), idle, kCFRunLoopCommonModes);
    NSApplication *app = [NSApplication sharedApplication];
    D *delegate = [D new];
    app.delegate = delegate;
    [app setActivationPolicy:NSApplicationActivationPolicyRegular];
    app.mainMenu = [NSMenu new];
    NSWindowStyleMask style = getenv("FLOOR_BORDERLESS")
        ? NSWindowStyleMaskBorderless
        : NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable |
              NSWindowStyleMaskMiniaturizable;
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
    [app run];
  }
  return 0;
}
