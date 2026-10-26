pub(super) fn show_error(message: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut std::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            flags: u32,
        ) -> i32;
    }
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "本地密码库".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0x10);
    }
}
