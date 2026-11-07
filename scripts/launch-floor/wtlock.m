// wtlock once|staged: how long the main thread waits while a worker loads
// WritingToolsUI (docs/decisions/launch-writing-tools-hold.md). `once` is one
// dlopen; `staged` opens one image per dlopen, dependencies first, as
// gpui-pre-macos/src/staged_load.rs does. A main-thread timer probes dyld's
// loader lock (NSVersionOfRunTimeLibrary) and the Objective-C runtime lock
// (class_copyMethodList) every millisecond until the load is done.
//
//   clang -O2 -fobjc-arc -framework AppKit wtlock.m -o wtlock
//   sudo purge; ./wtlock once; sudo purge; ./wtlock staged
#import <AppKit/AppKit.h>
#include <dlfcn.h>
#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <mach/mach_time.h>
#include <objc/runtime.h>
#include <pthread.h>

struct text_info {
    uint64_t version, loadAddressUnslid, textSegmentSize;
    uuid_t dylibUuid;
    const char *path;
    uint64_t textSegmentOffset;
};
extern int dyld_shared_cache_iterate_text(const uuid_t, void (^)(const struct text_info *));
extern bool _dyld_get_shared_cache_uuid(uuid_t);
extern const void *_dyld_get_shared_cache_range(size_t *);

static NSString *const WT = @"/System/Library/PrivateFrameworks/WritingToolsUI.framework/Versions/A/WritingToolsUI";
static uint64_t load_start, load_end;
static int opened;

static double ms(uint64_t t) {
    static mach_timebase_info_data_t tb;
    if (!tb.denom) mach_timebase_info(&tb);
    return (double)t * tb.numer / tb.denom / 1e6;
}

static NSArray<NSString *> *deps_of(const struct mach_header_64 *mh) {
    NSMutableArray *deps = [NSMutableArray array];
    const uint8_t *cmd = (const uint8_t *)(mh + 1);
    for (uint32_t i = 0; i < mh->ncmds; i++) {
        const struct load_command *lc = (const void *)cmd;
        if (lc->cmd == LC_LOAD_DYLIB || lc->cmd == LC_LOAD_WEAK_DYLIB || lc->cmd == LC_REEXPORT_DYLIB)
            [deps addObject:@((const char *)lc + ((const struct dylib_command *)lc)->dylib.name.offset)];
        cmd += lc->cmdsize;
    }
    return deps;
}

static NSMutableDictionary<NSString *, NSArray *> *graph;
static NSMutableArray<NSString *> *order;
static NSMutableSet<NSString *> *done;

static void visit(NSString *path) {
    if ([done containsObject:path]) return;
    [done addObject:path];
    for (NSString *dep in graph[path]) visit(dep);
    [order addObject:path];
}

// The images WT loads that are not loaded yet, each after what it links.
static NSArray<NSString *> *load_order(void) {
    uuid_t uuid;
    size_t len;
    _dyld_get_shared_cache_uuid(uuid);
    const uint8_t *base = _dyld_get_shared_cache_range(&len);
    NSMutableDictionary<NSString *, NSValue *> *headers = [NSMutableDictionary dictionary];
    dyld_shared_cache_iterate_text(uuid, ^(const struct text_info *info) {
        headers[@(info->path)] = [NSValue valueWithPointer:base + info->textSegmentOffset];
    });
    NSMutableSet *loaded = [NSMutableSet set];
    for (uint32_t i = 0; i < _dyld_image_count(); i++) [loaded addObject:@(_dyld_get_image_name(i))];
    graph = [NSMutableDictionary dictionary];
    NSMutableSet *queued = [NSMutableSet setWithObject:WT];
    NSArray *level = @[ WT ];
    while (level.count) {
        // Read the level's headers on several threads, so the disk reads overlap.
        dispatch_apply(level.count, dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^(size_t i) {
            const volatile struct mach_header_64 *mh = [headers[level[i]] pointerValue];
            if (mh) (void)((const volatile uint8_t *)(mh + 1))[mh->sizeofcmds ? mh->sizeofcmds - 1 : 0];
        });
        NSMutableArray *next = [NSMutableArray array];
        for (NSString *path in level) {
            const struct mach_header_64 *mh = [headers[path] pointerValue];
            NSMutableArray *deps = [NSMutableArray array];
            for (NSString *dep in mh ? deps_of(mh) : @[]) {
                if (!headers[dep] || [loaded containsObject:dep]) continue;
                [deps addObject:dep];
                if (![queued containsObject:dep]) { [queued addObject:dep]; [next addObject:dep]; }
            }
            graph[path] = deps;
        }
        level = next;
    }
    order = [NSMutableArray array];
    done = [NSMutableSet set];
    visit(WT);
    return order;
}

static void *worker(void *arg) {
    if (!strcmp(arg, "once")) {
        load_start = mach_absolute_time();
        dlopen(WT.UTF8String, RTLD_LAZY);
    } else {
        load_start = mach_absolute_time();
        for (NSString *path in load_order()) {
            dlopen(path.UTF8String, RTLD_LAZY);
            opened++;
        }
    }
    load_end = mach_absolute_time();
    return NULL;
}

int main(int argc, char **argv) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        if (dlopen(WT.UTF8String, RTLD_NOLOAD)) {
            fprintf(stderr, "WritingToolsUI is already loaded\n");
            return 1;
        }
        char *mode = argc > 1 ? argv[1] : "once";
        __block uint64_t dyld_worst = 0, objc_worst = 0, waited = 0, probes = 0;
        pthread_t thread;
        pthread_create(&thread, NULL, worker, mode);
        CFRunLoopTimerRef timer = CFRunLoopTimerCreateWithHandler(NULL, CFAbsoluteTimeGetCurrent(), 0.001, 0, 0, ^(CFRunLoopTimerRef t) {
            uint64_t a = mach_absolute_time();
            NSVersionOfRunTimeLibrary("Foundation");
            uint64_t b = mach_absolute_time();
            unsigned n;
            free(class_copyMethodList([NSView class], &n));
            uint64_t c = mach_absolute_time();
            probes++;
            dyld_worst = MAX(dyld_worst, b - a);
            objc_worst = MAX(objc_worst, c - b);
            if (ms(c - a) > 1) waited += c - a;
            if (load_end) {
                printf("%s: load %.0f ms, %d dlopens | main waited %.0f ms, longest dyld %.1f ms, objc %.1f ms, %llu probes\n",
                       mode, ms(load_end - load_start), opened, ms(waited), ms(dyld_worst), ms(objc_worst), probes);
                exit(0);
            }
        });
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
        CFRunLoopRun();
    }
}
