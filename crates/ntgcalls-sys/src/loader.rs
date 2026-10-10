//! The runtime `dlopen` loader that resolves every bound `ntg_*` symbol.
use super::*;

/// Runtime-loaded ntgcalls API.
///
/// `dlopen`s the LGPLv3 `libntgcalls.so` sidecar (procured unmodified by
/// `scripts/vendor-ntgcalls.sh`) and resolves every bound `ntg_*` symbol.
/// The `lib` field is kept alive so the resolved function pointers stay
/// valid for the `Loader`'s lifetime.
pub struct Loader {
    #[allow(dead_code)]
    lib: libloading::Library,
    pub ntg_set_log_callback: unsafe extern "C" fn(ntg_log_cb, *mut c_void),
    pub ntg_get_version: unsafe extern "C" fn() -> *const c_char,
    pub ntg_last_error: unsafe extern "C" fn() -> *const c_char,
    pub ntg_instance_create: unsafe extern "C" fn() -> *mut ntg_instance,
    pub ntg_instance_destroy: unsafe extern "C" fn(*mut ntg_instance),
    pub ntg_connection_info_free: unsafe extern "C" fn(*mut ntg_connection_info),
    pub ntg_remote_source_free: unsafe extern "C" fn(*mut ntg_remote_source),
    pub ntg_subchain_request_free: unsafe extern "C" fn(*mut ntg_subchain_request),
    pub ntg_audio_description_free: unsafe extern "C" fn(*mut ntg_audio_description),
    pub ntg_call_info_free: unsafe extern "C" fn(*mut ntg_call_info),
    pub ntg_device_info_free: unsafe extern "C" fn(*mut ntg_device_info),
    pub ntg_media_devices_free: unsafe extern "C" fn(*mut ntg_media_devices),
    pub ntg_media_state_free: unsafe extern "C" fn(*mut ntg_media_state),
    pub ntg_video_description_free: unsafe extern "C" fn(*mut ntg_video_description),
    pub ntg_media_description_free: unsafe extern "C" fn(*mut ntg_media_description),
    pub ntg_frame_data_free: unsafe extern "C" fn(*mut ntg_frame_data),
    pub ntg_ssrc_group_free: unsafe extern "C" fn(*mut ntg_ssrc_group),
    pub ntg_frame_free: unsafe extern "C" fn(*mut ntg_frame),
    pub ntg_segment_part_request_free: unsafe extern "C" fn(*mut ntg_segment_part_request),
    pub ntg_participants_request_free: unsafe extern "C" fn(*mut ntg_participants_request),
    pub ntg_ssrc_mapping_free: unsafe extern "C" fn(*mut ntg_ssrc_mapping),
    pub ntg_auth_params_free: unsafe extern "C" fn(*mut ntg_auth_params),
    pub ntg_conference_join_params_free: unsafe extern "C" fn(*mut ntg_conference_join_params),
    pub ntg_dh_config_free: unsafe extern "C" fn(*mut ntg_dh_config),
    pub ntg_protocol_free: unsafe extern "C" fn(*mut ntg_protocol),
    pub ntg_rtc_server_free: unsafe extern "C" fn(*mut ntg_rtc_server),
    pub ntg_call_info_entry_free: unsafe extern "C" fn(*mut ntg_call_info_entry, usize),
    pub ntg_string_free: unsafe extern "C" fn(*mut c_char),
    pub ntg_bytes_free: unsafe extern "C" fn(*mut c_void),
    pub ntg_create_p2p_call: unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result,
    pub ntg_init_exchange: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        ntg_dh_config,
        *const u8,
        usize,
        *mut *mut u8,
        *mut usize,
    ) -> ntg_result,
    pub ntg_exchange_keys: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        *const u8,
        usize,
        i64,
        *mut ntg_auth_params,
    ) -> ntg_result,
    pub ntg_skip_exchange:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize, bool) -> ntg_result,
    pub ntg_connect_p2p: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        *const ntg_rtc_server,
        usize,
        *const *const c_char,
        usize,
        bool,
        *const c_char,
    ) -> ntg_result,
    pub ntg_create_call:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result,
    pub ntg_init_presentation:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result,
    pub ntg_init_conference: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i64,
        *const u8,
        usize,
        *mut ntg_conference_join_params,
    ) -> ntg_result,
    pub ntg_connect:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const c_char, bool) -> ntg_result,
    pub ntg_add_incoming_video: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i64,
        *const c_char,
        *const ntg_ssrc_group,
        usize,
        *mut u32,
    ) -> ntg_result,
    pub ntg_remove_incoming_video:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const c_char, *mut bool) -> ntg_result,
    pub ntg_set_stream_sources: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        ntg_stream_mode,
        ntg_media_description,
    ) -> ntg_result,
    pub ntg_pause: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_resume: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_mute: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_unmute: unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result,
    pub ntg_stop: unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result,
    pub ntg_stop_presentation: unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result,
    pub ntg_get_emojis_fingerprint:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result,
    pub ntg_time:
        unsafe extern "C" fn(*mut ntg_instance, i64, ntg_stream_mode, *mut u64) -> ntg_result,
    pub ntg_get_state:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_media_state) -> ntg_result,
    pub ntg_get_call_type:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_call_type) -> ntg_result,
    pub ntg_get_connection_mode:
        unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_connection_mode) -> ntg_result,
    pub ntg_cpu_usage: unsafe extern "C" fn(*mut f64) -> ntg_result,
    pub ntg_ping: unsafe extern "C" fn(*mut *mut c_char) -> ntg_result,
    pub ntg_get_media_devices: unsafe extern "C" fn(*mut ntg_media_devices) -> ntg_result,
    pub ntg_get_protocol: unsafe extern "C" fn(*mut ntg_protocol) -> ntg_result,
    pub ntg_enable_glib_loop: unsafe extern "C" fn(bool) -> ntg_result,
    pub ntg_send_broadcast_timestamp:
        unsafe extern "C" fn(*mut ntg_instance, i64, i64) -> ntg_result,
    pub ntg_send_broadcast_part: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i64,
        i32,
        ntg_media_segment_part_status,
        bool,
        *const u8,
        usize,
    ) -> ntg_result,
    pub ntg_send_signaling_data:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize) -> ntg_result,
    pub ntg_send_external_frame: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        ntg_stream_device,
        *const u8,
        usize,
        ntg_frame_data,
    ) -> ntg_result,
    pub ntg_update_audio_ssrc_mappings:
        unsafe extern "C" fn(*mut ntg_instance, i64, *const ntg_ssrc_mapping, usize) -> ntg_result,
    pub ntg_apply_blocks: unsafe extern "C" fn(
        *mut ntg_instance,
        i64,
        i32,
        i32,
        *const ntg_bytes,
        usize,
        bool,
    ) -> ntg_result,
    pub ntg_finish_subchain_request:
        unsafe extern "C" fn(*mut ntg_instance, i64, i32) -> ntg_result,
    pub ntg_calls: unsafe extern "C" fn(
        *mut ntg_instance,
        *mut *mut ntg_call_info_entry,
        *mut usize,
    ) -> ntg_result,
    pub ntg_on_upgrade_callback:
        unsafe extern "C" fn(*mut ntg_instance, ntg_upgrade_callback_cb, *mut c_void) -> ntg_result,
    pub ntg_on_stream_end_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_stream_end_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_connection_change_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_connection_change_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_frames_callback:
        unsafe extern "C" fn(*mut ntg_instance, ntg_frames_callback_cb, *mut c_void) -> ntg_result,
    pub ntg_on_signaling_data_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_signaling_data_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_remote_source_change_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_remote_source_change_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_request_broadcast_part_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_request_broadcast_part_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_request_broadcast_timestamp_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_request_broadcast_timestamp_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_request_participants_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_request_participants_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_outbound_block_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_outbound_block_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_subchain_request_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_subchain_request_callback_cb,
        *mut c_void,
    ) -> ntg_result,
    pub ntg_on_update_emojis_callback: unsafe extern "C" fn(
        *mut ntg_instance,
        ntg_update_emojis_callback_cb,
        *mut c_void,
    ) -> ntg_result,
}

macro_rules! resolve {
    ($lib:expr, $name:literal, $ty:ty) => {{
        let name_bytes = concat!($name, "\0");
        let symbol: libloading::Symbol<$ty> = unsafe { $lib.get(name_bytes.as_bytes()) }
            .map_err(|_| LoadError::SymbolMissing { symbol: $name })?;
        *symbol
    }};
}

impl Loader {
    /// Open the library at `path` and resolve every bound `ntg_*` symbol.
    ///
    /// Fails with [`LoadError::LibraryMissing`] when the file cannot be
    /// opened, or [`LoadError::SymbolMissing`] naming the first symbol that
    /// did not resolve.
    pub fn load(path: impl AsRef<OsStr>) -> Result<Self, LoadError> {
        let path = path.as_ref();
        let lib =
            unsafe { libloading::Library::new(path) }.map_err(|_| LoadError::LibraryMissing {
                searched: vec![PathBuf::from(path)],
            })?;
        Ok(Self {
            ntg_set_log_callback: resolve!(
                lib,
                "ntg_set_log_callback",
                unsafe extern "C" fn(ntg_log_cb, *mut c_void)
            ),
            ntg_get_version: resolve!(
                lib,
                "ntg_get_version",
                unsafe extern "C" fn() -> *const c_char
            ),
            ntg_last_error: resolve!(
                lib,
                "ntg_last_error",
                unsafe extern "C" fn() -> *const c_char
            ),
            ntg_instance_create: resolve!(
                lib,
                "ntg_instance_create",
                unsafe extern "C" fn() -> *mut ntg_instance
            ),
            ntg_instance_destroy: resolve!(
                lib,
                "ntg_instance_destroy",
                unsafe extern "C" fn(*mut ntg_instance)
            ),
            ntg_connection_info_free: resolve!(
                lib,
                "ntg_connection_info_free",
                unsafe extern "C" fn(*mut ntg_connection_info)
            ),
            ntg_remote_source_free: resolve!(
                lib,
                "ntg_remote_source_free",
                unsafe extern "C" fn(*mut ntg_remote_source)
            ),
            ntg_subchain_request_free: resolve!(
                lib,
                "ntg_subchain_request_free",
                unsafe extern "C" fn(*mut ntg_subchain_request)
            ),
            ntg_audio_description_free: resolve!(
                lib,
                "ntg_audio_description_free",
                unsafe extern "C" fn(*mut ntg_audio_description)
            ),
            ntg_call_info_free: resolve!(
                lib,
                "ntg_call_info_free",
                unsafe extern "C" fn(*mut ntg_call_info)
            ),
            ntg_device_info_free: resolve!(
                lib,
                "ntg_device_info_free",
                unsafe extern "C" fn(*mut ntg_device_info)
            ),
            ntg_media_devices_free: resolve!(
                lib,
                "ntg_media_devices_free",
                unsafe extern "C" fn(*mut ntg_media_devices)
            ),
            ntg_media_state_free: resolve!(
                lib,
                "ntg_media_state_free",
                unsafe extern "C" fn(*mut ntg_media_state)
            ),
            ntg_video_description_free: resolve!(
                lib,
                "ntg_video_description_free",
                unsafe extern "C" fn(*mut ntg_video_description)
            ),
            ntg_media_description_free: resolve!(
                lib,
                "ntg_media_description_free",
                unsafe extern "C" fn(*mut ntg_media_description)
            ),
            ntg_frame_data_free: resolve!(
                lib,
                "ntg_frame_data_free",
                unsafe extern "C" fn(*mut ntg_frame_data)
            ),
            ntg_ssrc_group_free: resolve!(
                lib,
                "ntg_ssrc_group_free",
                unsafe extern "C" fn(*mut ntg_ssrc_group)
            ),
            ntg_frame_free: resolve!(lib, "ntg_frame_free", unsafe extern "C" fn(*mut ntg_frame)),
            ntg_segment_part_request_free: resolve!(
                lib,
                "ntg_segment_part_request_free",
                unsafe extern "C" fn(*mut ntg_segment_part_request)
            ),
            ntg_participants_request_free: resolve!(
                lib,
                "ntg_participants_request_free",
                unsafe extern "C" fn(*mut ntg_participants_request)
            ),
            ntg_ssrc_mapping_free: resolve!(
                lib,
                "ntg_ssrc_mapping_free",
                unsafe extern "C" fn(*mut ntg_ssrc_mapping)
            ),
            ntg_auth_params_free: resolve!(
                lib,
                "ntg_auth_params_free",
                unsafe extern "C" fn(*mut ntg_auth_params)
            ),
            ntg_conference_join_params_free: resolve!(
                lib,
                "ntg_conference_join_params_free",
                unsafe extern "C" fn(*mut ntg_conference_join_params)
            ),
            ntg_dh_config_free: resolve!(
                lib,
                "ntg_dh_config_free",
                unsafe extern "C" fn(*mut ntg_dh_config)
            ),
            ntg_protocol_free: resolve!(
                lib,
                "ntg_protocol_free",
                unsafe extern "C" fn(*mut ntg_protocol)
            ),
            ntg_rtc_server_free: resolve!(
                lib,
                "ntg_rtc_server_free",
                unsafe extern "C" fn(*mut ntg_rtc_server)
            ),
            ntg_call_info_entry_free: resolve!(
                lib,
                "ntg_call_info_entry_free",
                unsafe extern "C" fn(*mut ntg_call_info_entry, usize)
            ),
            ntg_string_free: resolve!(lib, "ntg_string_free", unsafe extern "C" fn(*mut c_char)),
            ntg_bytes_free: resolve!(lib, "ntg_bytes_free", unsafe extern "C" fn(*mut c_void)),
            ntg_create_p2p_call: resolve!(
                lib,
                "ntg_create_p2p_call",
                unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result
            ),
            ntg_init_exchange: resolve!(
                lib,
                "ntg_init_exchange",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_dh_config,
                    *const u8,
                    usize,
                    *mut *mut u8,
                    *mut usize,
                ) -> ntg_result
            ),
            ntg_exchange_keys: resolve!(
                lib,
                "ntg_exchange_keys",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const u8,
                    usize,
                    i64,
                    *mut ntg_auth_params,
                ) -> ntg_result
            ),
            ntg_skip_exchange: resolve!(
                lib,
                "ntg_skip_exchange",
                unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize, bool) -> ntg_result
            ),
            ntg_connect_p2p: resolve!(
                lib,
                "ntg_connect_p2p",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const ntg_rtc_server,
                    usize,
                    *const *const c_char,
                    usize,
                    bool,
                    *const c_char,
                ) -> ntg_result
            ),
            ntg_create_call: resolve!(
                lib,
                "ntg_create_call",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result
            ),
            ntg_init_presentation: resolve!(
                lib,
                "ntg_init_presentation",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result
            ),
            ntg_init_conference: resolve!(
                lib,
                "ntg_init_conference",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i64,
                    *const u8,
                    usize,
                    *mut ntg_conference_join_params,
                ) -> ntg_result
            ),
            ntg_connect: resolve!(
                lib,
                "ntg_connect",
                unsafe extern "C" fn(*mut ntg_instance, i64, *const c_char, bool) -> ntg_result
            ),
            ntg_add_incoming_video: resolve!(
                lib,
                "ntg_add_incoming_video",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i64,
                    *const c_char,
                    *const ntg_ssrc_group,
                    usize,
                    *mut u32,
                ) -> ntg_result
            ),
            ntg_remove_incoming_video: resolve!(
                lib,
                "ntg_remove_incoming_video",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const c_char,
                    *mut bool,
                ) -> ntg_result
            ),
            ntg_set_stream_sources: resolve!(
                lib,
                "ntg_set_stream_sources",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_stream_mode,
                    ntg_media_description,
                ) -> ntg_result
            ),
            ntg_pause: resolve!(
                lib,
                "ntg_pause",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_resume: resolve!(
                lib,
                "ntg_resume",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_mute: resolve!(
                lib,
                "ntg_mute",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_unmute: resolve!(
                lib,
                "ntg_unmute",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut bool) -> ntg_result
            ),
            ntg_stop: resolve!(
                lib,
                "ntg_stop",
                unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result
            ),
            ntg_stop_presentation: resolve!(
                lib,
                "ntg_stop_presentation",
                unsafe extern "C" fn(*mut ntg_instance, i64) -> ntg_result
            ),
            ntg_get_emojis_fingerprint: resolve!(
                lib,
                "ntg_get_emojis_fingerprint",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut *mut c_char) -> ntg_result
            ),
            ntg_time: resolve!(
                lib,
                "ntg_time",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_stream_mode,
                    *mut u64,
                ) -> ntg_result
            ),
            ntg_get_state: resolve!(
                lib,
                "ntg_get_state",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_media_state) -> ntg_result
            ),
            ntg_get_call_type: resolve!(
                lib,
                "ntg_get_call_type",
                unsafe extern "C" fn(*mut ntg_instance, i64, *mut ntg_call_type) -> ntg_result
            ),
            ntg_get_connection_mode: resolve!(
                lib,
                "ntg_get_connection_mode",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *mut ntg_connection_mode,
                ) -> ntg_result
            ),
            ntg_cpu_usage: resolve!(
                lib,
                "ntg_cpu_usage",
                unsafe extern "C" fn(*mut f64) -> ntg_result
            ),
            ntg_ping: resolve!(
                lib,
                "ntg_ping",
                unsafe extern "C" fn(*mut *mut c_char) -> ntg_result
            ),
            ntg_get_media_devices: resolve!(
                lib,
                "ntg_get_media_devices",
                unsafe extern "C" fn(*mut ntg_media_devices) -> ntg_result
            ),
            ntg_get_protocol: resolve!(
                lib,
                "ntg_get_protocol",
                unsafe extern "C" fn(*mut ntg_protocol) -> ntg_result
            ),
            ntg_enable_glib_loop: resolve!(
                lib,
                "ntg_enable_glib_loop",
                unsafe extern "C" fn(bool) -> ntg_result
            ),
            ntg_send_broadcast_timestamp: resolve!(
                lib,
                "ntg_send_broadcast_timestamp",
                unsafe extern "C" fn(*mut ntg_instance, i64, i64) -> ntg_result
            ),
            ntg_send_broadcast_part: resolve!(
                lib,
                "ntg_send_broadcast_part",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i64,
                    i32,
                    ntg_media_segment_part_status,
                    bool,
                    *const u8,
                    usize,
                ) -> ntg_result
            ),
            ntg_send_signaling_data: resolve!(
                lib,
                "ntg_send_signaling_data",
                unsafe extern "C" fn(*mut ntg_instance, i64, *const u8, usize) -> ntg_result
            ),
            ntg_send_external_frame: resolve!(
                lib,
                "ntg_send_external_frame",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    ntg_stream_device,
                    *const u8,
                    usize,
                    ntg_frame_data,
                ) -> ntg_result
            ),
            ntg_update_audio_ssrc_mappings: resolve!(
                lib,
                "ntg_update_audio_ssrc_mappings",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    *const ntg_ssrc_mapping,
                    usize,
                ) -> ntg_result
            ),
            ntg_apply_blocks: resolve!(
                lib,
                "ntg_apply_blocks",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    i64,
                    i32,
                    i32,
                    *const ntg_bytes,
                    usize,
                    bool,
                ) -> ntg_result
            ),
            ntg_finish_subchain_request: resolve!(
                lib,
                "ntg_finish_subchain_request",
                unsafe extern "C" fn(*mut ntg_instance, i64, i32) -> ntg_result
            ),
            ntg_calls: resolve!(
                lib,
                "ntg_calls",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    *mut *mut ntg_call_info_entry,
                    *mut usize,
                ) -> ntg_result
            ),
            ntg_on_upgrade_callback: resolve!(
                lib,
                "ntg_on_upgrade_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_upgrade_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_stream_end_callback: resolve!(
                lib,
                "ntg_on_stream_end_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_stream_end_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_connection_change_callback: resolve!(
                lib,
                "ntg_on_connection_change_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_connection_change_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_frames_callback: resolve!(
                lib,
                "ntg_on_frames_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_frames_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_signaling_data_callback: resolve!(
                lib,
                "ntg_on_signaling_data_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_signaling_data_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_remote_source_change_callback: resolve!(
                lib,
                "ntg_on_remote_source_change_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_remote_source_change_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_request_broadcast_part_callback: resolve!(
                lib,
                "ntg_on_request_broadcast_part_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_request_broadcast_part_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_request_broadcast_timestamp_callback: resolve!(
                lib,
                "ntg_on_request_broadcast_timestamp_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_request_broadcast_timestamp_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_request_participants_callback: resolve!(
                lib,
                "ntg_on_request_participants_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_request_participants_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_outbound_block_callback: resolve!(
                lib,
                "ntg_on_outbound_block_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_outbound_block_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_subchain_request_callback: resolve!(
                lib,
                "ntg_on_subchain_request_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_subchain_request_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            ntg_on_update_emojis_callback: resolve!(
                lib,
                "ntg_on_update_emojis_callback",
                unsafe extern "C" fn(
                    *mut ntg_instance,
                    ntg_update_emojis_callback_cb,
                    *mut c_void,
                ) -> ntg_result
            ),
            lib,
        })
    }

    /// Search the standard locations for the sidecar library and load it.
    ///
    /// Order: `QUILL_NTGCALLS_LIB` env var, then
    /// The native library under `vendor/ntgcalls/lib` at any ancestor of the current
    /// executable (covers `cargo run` from the repo), then the bare
    /// native library filename on the system search path.
    pub fn load_default() -> Result<Self, LoadError> {
        let mut searched: Vec<PathBuf> = Vec::new();

        if let Some(env_path) = std::env::var_os("QUILL_NTGCALLS_LIB") {
            let path = PathBuf::from(env_path);
            searched.push(path.clone());
            if let Ok(loader) = Self::load(&path) {
                return Ok(loader);
            }
        }

        if let Ok(exe) = std::env::current_exe() {
            // Linux/Windows package layout: `<dir>/quill` + `<dir>/lib/<native lib>`.
            if let Some(candidate) = exe
                .parent()
                .map(|dir| dir.join("lib").join(library_filename()))
                .filter(|candidate| candidate.exists())
            {
                return Self::load(&candidate);
            }
            for ancestor in exe
                .parent()
                .into_iter()
                .flat_map(std::path::Path::ancestors)
            {
                for directory in [
                    ancestor.join("Frameworks"),
                    ancestor.join("vendor/ntgcalls/lib"),
                    // Windows: the package keeps ntgcalls.dll beside quill.exe; the
                    // upstream zip puts it under lib/Release.
                    #[cfg(windows)]
                    ancestor.to_path_buf(),
                    #[cfg(windows)]
                    ancestor.join("vendor/ntgcalls/lib/Release"),
                ] {
                    let candidate = directory.join(library_filename());
                    if candidate.exists() {
                        return Self::load(&candidate);
                    }
                }
            }
        }

        let system = PathBuf::from(library_filename());
        searched.push(system.clone());
        Self::load(&system).map_err(|_| LoadError::LibraryMissing { searched })
    }
}
