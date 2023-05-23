use anyhow::{Context, Error};
use arboard::Clipboard;
use rfd::{MessageButtons, MessageDialog, MessageLevel};
use ruffle_core::backend::ui::{
    FullscreenError, LanguageIdentifier, MouseCursor, UiBackend, US_ENGLISH,
};
use std::rc::Rc;
use sys_locale::get_locale;
use tracing::error;
use winit::window::{Fullscreen, Window};

pub struct DesktopUiBackend {
    window: Rc<Window>,
    cursor_visible: bool,
    clipboard: Clipboard,
    language: LanguageIdentifier,
}

impl DesktopUiBackend {
    pub fn new(window: Rc<Window>) -> Result<Self, Error> {
        let preferred_language = get_locale();
        let language = preferred_language
            .and_then(|l| l.parse().ok())
            .unwrap_or_else(|| US_ENGLISH.clone());
        Ok(Self {
            window,
            cursor_visible: true,
            clipboard: Clipboard::new().context("Couldn't get platform clipboard")?,
            language,
        })
    }
}

// TODO: Move link to https://ruffle.rs/faq or similar
const UNSUPPORTED_CONTENT_MESSAGE: &str = "\
The Ruffle emulator may not yet fully support all of ActionScript 3 used by this content.
Some parts of the content may not work as expected.

See the following link for more info:
https://github.com/ruffle-rs/ruffle/wiki/Frequently-Asked-Questions-For-Users";

const DOWNLOAD_FAILED_MESSAGE: &str = "Ruffle failed to open or download this file.";

impl UiBackend for DesktopUiBackend {
    fn mouse_visible(&self) -> bool {
        self.cursor_visible
    }

    fn set_mouse_visible(&mut self, visible: bool) {
        self.window.set_cursor_visible(visible);
        self.cursor_visible = visible;
    }

    fn set_mouse_cursor(&mut self, cursor: MouseCursor) {
        use winit::window::CursorIcon;
        let icon = match cursor {
            MouseCursor::Arrow => CursorIcon::Arrow,
            MouseCursor::Hand => CursorIcon::Hand,
            MouseCursor::IBeam => CursorIcon::Text,
            MouseCursor::Grab => CursorIcon::Grab,
        };
        self.window.set_cursor_icon(icon);
    }

    fn clipboard_content(&mut self) -> String {
        self.clipboard.get_text().unwrap_or_default()
    }

    fn set_clipboard_content(&mut self, content: String) {
        if let Err(e) = self.clipboard.set_text(content) {
            error!("Couldn't set clipboard contents: {:?}", e);
        }
    }

    fn set_fullscreen(&mut self, is_full: bool) -> Result<(), FullscreenError> {
        self.window.set_fullscreen(if is_full {
            Some(Fullscreen::Borderless(None))
        } else {
            None
        });
        Ok(())
    }

    fn display_unsupported_message(&self) {
        let dialog = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Ruffle - Unsupported content")
            .set_description(UNSUPPORTED_CONTENT_MESSAGE)
            .set_buttons(MessageButtons::Ok);
        dialog.show();
    }

    fn display_root_movie_download_failed_message(&self) {
        let dialog = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Ruffle - Load failed")
            .set_description(DOWNLOAD_FAILED_MESSAGE)
            .set_buttons(MessageButtons::Ok);
        dialog.show();
    }

    fn message(&self, message: &str) {
        let dialog = MessageDialog::new()
            .set_level(MessageLevel::Info)
            .set_title("Ruffle")
            .set_description(message)
            .set_buttons(MessageButtons::Ok);
        dialog.show();
    }

    // Unused on desktop
    fn open_virtual_keyboard(&self) {}

    fn language(&self) -> &LanguageIdentifier {
        &self.language
    }
}
