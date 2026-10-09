// Camera QR scanning uses Apple's capture and barcode APIs. The helper has no
// account credentials; stdout carries one candidate only to the parent pipe.
#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <CoreImage/CoreImage.h>
#import <Vision/Vision.h>
#include <unistd.h>

static NSString *loginQR(VNDetectBarcodesRequest *request) {
    for (VNBarcodeObservation *result in request.results) {
        NSString *value = result.payloadStringValue;
        if ([value hasPrefix:@"tg://login?token="] && value.length <= 1500) return value;
    }
    return nil;
}

@interface Scanner : NSObject <NSApplicationDelegate, NSWindowDelegate, AVCaptureVideoDataOutputSampleBufferDelegate>
@property NSWindow *window;
@property NSTextField *notice;
@property NSView *preview;
@property AVCaptureSession *capture;
@property dispatch_queue_t queue;
@property BOOL found;
@end

@implementation Scanner
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    self.queue = dispatch_queue_create("org.shinycake.quill.qr-camera", DISPATCH_QUEUE_SERIAL);
    self.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 640, 460)
        styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable
        backing:NSBackingStoreBuffered defer:NO];
    self.window.title = @"Link a device — Quill";
    self.window.contentMinSize = NSMakeSize(480, 380);
    self.window.delegate = self;
    self.notice = [NSTextField wrappingLabelWithString:@"Point your camera at the login QR code on the other device. Quill will ask you to confirm before linking it."];
    self.preview = [[NSView alloc] init];
    self.preview.wantsLayer = YES;
    [self.preview setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationVertical];
    [self.preview setAccessibilityLabel:@"Camera preview"];
    NSButton *cancel = [NSButton buttonWithTitle:@"Cancel" target:self action:@selector(cancel:)];
    cancel.keyEquivalent = @"\033";
    NSStackView *stack = [NSStackView stackViewWithViews:@[self.notice, self.preview, cancel]];
    stack.orientation = NSUserInterfaceLayoutOrientationVertical;
    stack.alignment = NSLayoutAttributeLeading;
    stack.spacing = 16;
    stack.translatesAutoresizingMaskIntoConstraints = NO;
    [self.window.contentView addSubview:stack];
    [NSLayoutConstraint activateConstraints:@[
        [stack.leadingAnchor constraintEqualToAnchor:self.window.contentView.leadingAnchor constant:20],
        [stack.trailingAnchor constraintEqualToAnchor:self.window.contentView.trailingAnchor constant:-20],
        [stack.topAnchor constraintEqualToAnchor:self.window.contentView.topAnchor constant:20],
        [stack.bottomAnchor constraintEqualToAnchor:self.window.contentView.bottomAnchor constant:-20],
        [self.preview.widthAnchor constraintEqualToAnchor:stack.widthAnchor],
        [self.preview.heightAnchor constraintGreaterThanOrEqualToConstant:200]
    ]];
    [self.window center];
    [self.window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
    AVAuthorizationStatus status = [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeVideo];
    if (status == AVAuthorizationStatusNotDetermined) {
        [AVCaptureDevice requestAccessForMediaType:AVMediaTypeVideo completionHandler:^(BOOL granted) {
            dispatch_async(dispatch_get_main_queue(), ^{ [self cameraPermission:granted]; });
        }];
    } else {
        [self cameraPermission:status == AVAuthorizationStatusAuthorized];
    }
}
- (void)cameraPermission:(BOOL)granted {
    if (!granted) {
        self.notice.stringValue = @"Camera access is off. Enable Quill in System Settings → Privacy & Security → Camera, then try again.";
        return;
    }
    self.capture = [[AVCaptureSession alloc] init];
    AVCaptureVideoPreviewLayer *layer = [AVCaptureVideoPreviewLayer layerWithSession:self.capture];
    layer.videoGravity = AVLayerVideoGravityResizeAspectFill;
    layer.frame = self.preview.bounds;
    layer.autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable;
    [self.preview.layer addSublayer:layer];
    dispatch_async(self.queue, ^{
        AVCaptureDevice *device = [AVCaptureDevice defaultDeviceWithMediaType:AVMediaTypeVideo];
        NSError *error = nil;
        AVCaptureDeviceInput *input = device ? [AVCaptureDeviceInput deviceInputWithDevice:device error:&error] : nil;
        AVCaptureVideoDataOutput *output = [[AVCaptureVideoDataOutput alloc] init];
        output.alwaysDiscardsLateVideoFrames = YES;
        output.videoSettings = @{(id)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA)};
        [output setSampleBufferDelegate:self queue:self.queue];
        [self.capture beginConfiguration];
        BOOL usable = input && [self.capture canAddInput:input];
        if (usable) [self.capture addInput:input];
        usable = usable && [self.capture canAddOutput:output];
        if (usable) [self.capture addOutput:output];
        [self.capture commitConfiguration];
        if (usable) {
            [self.capture startRunning];
        } else {
            dispatch_async(dispatch_get_main_queue(), ^{ self.notice.stringValue = @"The camera couldn't start. Check that a camera is connected and available, then try again."; });
        }
    });
}
- (void)captureOutput:(AVCaptureOutput *)output didOutputSampleBuffer:(CMSampleBufferRef)sample fromConnection:(AVCaptureConnection *)connection {
    if (self.found) return;
    @autoreleasepool {
        VNDetectBarcodesRequest *request = [[VNDetectBarcodesRequest alloc] init];
        request.symbologies = @[VNBarcodeSymbologyQR];
        VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCMSampleBuffer:sample options:@{}];
        if (![handler performRequests:@[request] error:nil]) return;
        NSString *value = loginQR(request);
        if (!value) return;
        self.found = YES;
        // Process exit releases capture before the parent presents confirmation.
        NSData *data = [value dataUsingEncoding:NSUTF8StringEncoding];
        fwrite(data.bytes, 1, data.length, stdout);
        fflush(stdout);
        dispatch_async(dispatch_get_main_queue(), ^{ exit(0); });
    }
}
- (void)cancel:(id)sender { exit(2); }
- (void)windowWillClose:(NSNotification *)notification { exit(2); }
@end

static int selfTest(void) {
    NSString *fixture = @"tg://login?token=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    CIFilter *filter = [CIFilter filterWithName:@"CIQRCodeGenerator"];
    [filter setValue:[fixture dataUsingEncoding:NSUTF8StringEncoding] forKey:@"inputMessage"];
    CIImage *image = [filter.outputImage imageByApplyingTransform:CGAffineTransformMakeScale(10, 10)];
    CIContext *context = [CIContext contextWithOptions:nil];
    CGImageRef bitmap = [context createCGImage:image fromRect:image.extent];
    VNDetectBarcodesRequest *request = [[VNDetectBarcodesRequest alloc] init];
    request.symbologies = @[VNBarcodeSymbologyQR];
    VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCGImage:bitmap options:@{}];
    NSError *error = nil;
    BOOL ran = [handler performRequests:@[request] error:&error];
    NSString *decoded = ran ? loginQR(request) : nil;
    CGImageRelease(bitmap);
    if (!ran) {
        fprintf(stderr, "FAIL: Vision barcode request failed: %s\n",
                error.localizedDescription.UTF8String ?: "unknown error");
        return 1;
    }
    if (![decoded isEqualToString:fixture]) {
        fprintf(stderr, "FAIL: Vision decoded %s instead of the fixture\n",
                decoded ? "a different payload" : "no QR code");
        return 1;
    }
    puts("PASS: native QR generation and Vision decoding; camera and account unused");
    return 0;
}
int main(int argc, const char *argv[]) {
    @autoreleasepool {
        if (argc == 2 && strcmp(argv[1], "--self-test") == 0) return selfTest();
        if (argc != 1) return 1;
        NSApplication *app = [NSApplication sharedApplication];
        [app setActivationPolicy:NSApplicationActivationPolicyAccessory];
        pid_t parent = getppid();
        [NSTimer scheduledTimerWithTimeInterval:0.5 repeats:YES block:^(NSTimer *timer) {
            if (getppid() != parent) exit(2);
        }];
        __attribute__((objc_precise_lifetime)) Scanner *scanner = [[Scanner alloc] init];
        app.delegate = scanner;
        [app run];
    }
    return 2;
}
