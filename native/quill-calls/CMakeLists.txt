# libquill_calls: Telegram's tgcalls behind Quill's C API (include/quill_calls.h).
# Built by scripts/build-tgcalls.sh, which passes the dependency locations.
# The tgcalls source list and compile definitions mirror Telegram Desktop's
# Telegram/cmake/lib_tgcalls.cmake at the pinned commit.
cmake_minimum_required(VERSION 3.21)
project(quill_calls LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_VISIBILITY_INLINES_HIDDEN ON)
if (APPLE)
    enable_language(OBJC OBJCXX)
endif()

set(QUILL_CALLS_DEPS "" CACHE PATH "Prefix with tg_owt, Opus, libvpx, OpenH264, libjpeg and rnnoise")
set(QUILL_CALLS_TGCALLS "" CACHE PATH "tgcalls checkout")
set(QUILL_CALLS_FFMPEG "" CACHE PATH "FFmpeg prefix (scripts/build-ffmpeg.sh)")
set(QUILL_CALLS_OPENSSL_INCLUDE "" CACHE PATH "OpenSSL 3 include directory")
foreach (var QUILL_CALLS_DEPS QUILL_CALLS_TGCALLS QUILL_CALLS_FFMPEG QUILL_CALLS_OPENSSL_INCLUDE)
    if ("${${var}}" STREQUAL "")
        message(FATAL_ERROR "${var} is required; run scripts/build-tgcalls.sh")
    endif()
endforeach()

find_package(tg_owt REQUIRED PATHS ${QUILL_CALLS_DEPS} NO_DEFAULT_PATH)
find_package(OpenSSL 3 REQUIRED)
find_package(ZLIB REQUIRED)

set(tgcalls_loc ${QUILL_CALLS_TGCALLS}/tgcalls)

set(tgcalls_sources
    Instance.cpp
    AudioDeviceHelper.cpp
    ChannelManager.cpp
    CodecSelectHelper.cpp
    CryptoHelper.cpp
    EncryptedConnection.cpp
    FakeAudioDeviceModule.cpp
    FieldTrialsConfig.cpp
    InstanceImpl.cpp
    LogSinkImpl.cpp
    Manager.cpp
    MediaManager.cpp
    Message.cpp
    NetworkManager.cpp
    SctpDataChannelProviderInterfaceImpl.cpp
    StaticThreads.cpp
    TurnCustomizerImpl.cpp
    VideoCaptureInterface.cpp
    VideoCaptureInterfaceImpl.cpp
    utils/gzip.cpp
    v2/ContentNegotiation.cpp
    v2/CustomDcSctpSocket.cpp
    v2/DirectNetworkingImpl.cpp
    v2/ExternalSignalingConnection.cpp
    v2/InstanceV2ReferenceImpl.cpp
    v2/InstanceV2Impl.cpp
    v2/MtProtoIceTransport.cpp
    v2/NativeNetworkingImpl.cpp
    v2/RawTcpSocket.cpp
    v2/ReflectorPort.cpp
    v2/ReflectorRelayPortFactory.cpp
    v2/Signaling.cpp
    v2/SignalingConnection.cpp
    v2/SignalingEncryption.cpp
    v2/SignalingSctpConnection.cpp
    desktop_capturer/DesktopCaptureSource.cpp
    desktop_capturer/DesktopCaptureSourceHelper.cpp
    desktop_capturer/DesktopCaptureSourceManager.cpp
    group/AVIOContextImpl.cpp
    group/AudioStreamingPart.cpp
    group/AudioStreamingPartInternal.cpp
    group/AudioStreamingPartPersistentDecoder.cpp
    group/GroupInstanceCustomImpl.cpp
    group/GroupJoinPayloadInternal.cpp
    group/GroupNetworkManager.cpp
    group/StreamingAudioRenderer.cpp
    group/StreamingMediaContext.cpp
    group/VideoStreamingPart.cpp
    third-party/json11.cpp
)

if (APPLE)
    # tdesktop on macOS: the darwin platform layer minus the iOS camera and
    # Metal views, and none of platform/tdesktop.
    list(APPEND tgcalls_sources
        platform/darwin/CustomSimulcastEncoderAdapter.cpp
        platform/darwin/DarwinFFMpeg.mm
        platform/darwin/DarwinInterface.mm
        platform/darwin/DarwinVideoSource.mm
        platform/darwin/DesktopSharingCapturer.mm
        platform/darwin/ExtractCVPixelBuffer.mm
        platform/darwin/h265_nalu_rewriter.cc
        platform/darwin/objc_video_encoder_factory.mm
        platform/darwin/objc_video_decoder_factory.mm
        platform/darwin/RTCCodecSpecificInfoH265.mm
        platform/darwin/RTCH265ProfileLevelId.mm
        platform/darwin/TGCMIOCapturer.m
        platform/darwin/TGCMIODevice.mm
        platform/darwin/TGRTCCVPixelBuffer.mm
        platform/darwin/TGRTCDefaultVideoDecoderFactory.mm
        platform/darwin/TGRTCDefaultVideoEncoderFactory.mm
        platform/darwin/TGRTCVideoDecoderH264.mm
        platform/darwin/TGRTCVideoDecoderH265.mm
        platform/darwin/TGRTCVideoEncoderH264.mm
        platform/darwin/TGRTCVideoEncoderH265.mm
        platform/darwin/VideoCameraCapturerMac.mm
        platform/darwin/VideoCapturerInterfaceImpl.mm
        platform/darwin/VideoCMIOCapture.mm
    )
else()
    list(APPEND tgcalls_sources
        platform/tdesktop/DesktopInterface.cpp
        platform/tdesktop/VideoCapturerInterfaceImpl.cpp
        platform/tdesktop/VideoCapturerTrackSource.cpp
        platform/tdesktop/VideoCameraCapturer.cpp
    )
endif()
list(TRANSFORM tgcalls_sources PREPEND ${tgcalls_loc}/)

add_library(quill_calls SHARED
    src/quill_calls.cpp
    ${tgcalls_sources}
)

target_include_directories(quill_calls
PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/include
PRIVATE
    ${QUILL_CALLS_TGCALLS}
    ${tgcalls_loc}
    ${QUILL_CALLS_DEPS}/include
    ${QUILL_CALLS_FFMPEG}/include
    ${QUILL_CALLS_OPENSSL_INCLUDE}
)

target_compile_definitions(quill_calls
PRIVATE
    QUILL_CALLS_BUILD
    TGCALLS_USE_STD_OPTIONAL
    WEBRTC_APP_TDESKTOP
    RTC_ENABLE_H265
    RTC_ENABLE_VP9
)

if (APPLE)
    target_compile_options(quill_calls PRIVATE $<$<COMPILE_LANGUAGE:OBJC,OBJCXX>:-fobjc-arc>)
endif()
if (NOT MSVC)
    target_compile_options(quill_calls PRIVATE
        -Wno-deprecated-volatile
        -Wno-deprecated-declarations
        -Wno-unused-function
        $<$<CXX_COMPILER_ID:Clang,AppleClang>:-Wno-ambiguous-reversed-operator -Wno-unqualified-std-cast-call>
    )
endif()

find_library(FFMPEG_AVCODEC avcodec PATHS ${QUILL_CALLS_FFMPEG}/lib NO_DEFAULT_PATH REQUIRED)
find_library(FFMPEG_AVFORMAT avformat PATHS ${QUILL_CALLS_FFMPEG}/lib NO_DEFAULT_PATH REQUIRED)
find_library(FFMPEG_AVUTIL avutil PATHS ${QUILL_CALLS_FFMPEG}/lib NO_DEFAULT_PATH REQUIRED)
find_library(FFMPEG_SWRESAMPLE swresample PATHS ${QUILL_CALLS_FFMPEG}/lib NO_DEFAULT_PATH REQUIRED)
find_library(FFMPEG_SWSCALE swscale PATHS ${QUILL_CALLS_FFMPEG}/lib NO_DEFAULT_PATH REQUIRED)

target_link_libraries(quill_calls
PRIVATE
    tg_owt::tg_owt
    ${QUILL_CALLS_DEPS}/lib/libopus.a
    ${QUILL_CALLS_DEPS}/lib/libvpx.a
    ${QUILL_CALLS_DEPS}/lib/libopenh264.a
    ${QUILL_CALLS_DEPS}/lib/libjpeg.a
    ${QUILL_CALLS_DEPS}/lib/librnnoise.a
    ${FFMPEG_AVFORMAT}
    ${FFMPEG_AVCODEC}
    ${FFMPEG_SWRESAMPLE}
    ${FFMPEG_SWSCALE}
    ${FFMPEG_AVUTIL}
    OpenSSL::SSL
    OpenSSL::Crypto
    ZLIB::ZLIB
)

if (APPLE)
    target_link_libraries(quill_calls PRIVATE
        "-framework AVFoundation"
        "-framework AudioToolbox"
        "-framework CoreAudio"
        "-framework CoreMedia"
        "-framework CoreVideo"
        "-framework CoreMediaIO"
        "-framework VideoToolbox"
        "-framework Foundation"
        "-framework AppKit"
        "-framework IOSurface"
        "-framework Metal"
        "-framework ScreenCaptureKit"
        "-framework ApplicationServices"
    )
    # Everything we link statically stays private; only qc_* is exported.
    target_link_options(quill_calls PRIVATE "-Wl,-exported_symbol,_qc_*")
    # FFmpeg ships next to the library, so it is found from there.
    set_target_properties(quill_calls PROPERTIES
        INSTALL_NAME_DIR "@rpath"
        INSTALL_RPATH "@loader_path"
        BUILD_WITH_INSTALL_RPATH ON)
else()
    find_package(Threads REQUIRED)
    # tg_owt's X11 and portal (GLib) capture code, as tdesktop links it on
    # Linux; PipeWire itself is loaded with dlopen.
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(QC_SYSTEM REQUIRED IMPORTED_TARGET
        gio-2.0 gobject-2.0 glib-2.0
        x11 xcomposite xdamage xext xfixes xrandr xrender xtst)
    target_link_libraries(quill_calls PRIVATE Threads::Threads PkgConfig::QC_SYSTEM ${CMAKE_DL_LIBS})
    # Everything we link statically stays private; only qc_* is exported.
    target_link_options(quill_calls PRIVATE
        -Wl,--exclude-libs,ALL
        -Wl,--no-undefined
        -Wl,--version-script=${CMAKE_CURRENT_SOURCE_DIR}/exports.map)
    set_target_properties(quill_calls PROPERTIES LINK_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/exports.map)
    set_target_properties(quill_calls PROPERTIES
        INSTALL_RPATH "$ORIGIN"
        BUILD_WITH_INSTALL_RPATH ON)
endif()

install(TARGETS quill_calls LIBRARY DESTINATION lib RUNTIME DESTINATION bin ARCHIVE DESTINATION lib)
install(FILES include/quill_calls.h DESTINATION include)
