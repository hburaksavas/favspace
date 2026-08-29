use image::{codecs::png::PngEncoder, ExtendedColorType, ImageEncoder};
use std::{ffi::c_void, mem::size_of, path::Path};
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::SIZE,
        Graphics::Gdi::{
            DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BI_RGB,
            DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
        },
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
        UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK},
    },
};

struct BitmapGuard(HBITMAP);

impl Drop for BitmapGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.0 .0));
        }
    }
}

struct ComGuard(bool);

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

pub fn extract_png(path: &Path, requested_size: i32) -> Result<Vec<u8>, String> {
    unsafe {
        let _com_guard = ComGuard(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let parsing_name = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&parsing_name, None)
            .map_err(|error| format!("Shell öğesi oluşturulamadı: {error}"))?;
        let bitmap = BitmapGuard(
            factory
                .GetImage(
                    SIZE {
                        cx: requested_size,
                        cy: requested_size,
                    },
                    SIIGBF_BIGGERSIZEOK,
                )
                .map_err(|error| format!("Shell görseli alınamadı: {error}"))?,
        );

        let mut bitmap_info = BITMAP::default();
        if GetObjectW(
            HGDIOBJ(bitmap.0 .0),
            size_of::<BITMAP>() as i32,
            Some((&mut bitmap_info as *mut BITMAP).cast::<c_void>()),
        ) == 0
        {
            return Err("Shell bitmap bilgisi okunamadı.".into());
        }
        let width = bitmap_info.bmWidth.unsigned_abs();
        let height = bitmap_info.bmHeight.unsigned_abs();
        if width == 0 || height == 0 || width > 1024 || height > 1024 {
            return Err("Shell geçersiz bir görsel boyutu döndürdü.".into());
        }

        let mut dib = BITMAPINFO::default();
        dib.bmiHeader.biSize = size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        dib.bmiHeader.biWidth = width as i32;
        dib.bmiHeader.biHeight = -(height as i32);
        dib.bmiHeader.biPlanes = 1;
        dib.bmiHeader.biBitCount = 32;
        dib.bmiHeader.biCompression = BI_RGB.0;

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let device_context = GetDC(None);
        if device_context.is_invalid() {
            return Err("Windows device context alınamadı.".into());
        }
        let copied = GetDIBits(
            device_context,
            bitmap.0,
            0,
            height,
            Some(pixels.as_mut_ptr().cast::<c_void>()),
            &mut dib,
            DIB_RGB_COLORS,
        );
        let _ = ReleaseDC(None, device_context);
        if copied == 0 {
            return Err("Shell bitmap pikselleri okunamadı.".into());
        }

        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
                pixel[3] = 255;
            }
        }
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&pixels, width, height, ExtendedColorType::Rgba8)
            .map_err(|error| format!("Shell görseli PNG olarak kodlanamadı: {error}"))?;
        Ok(png)
    }
}
