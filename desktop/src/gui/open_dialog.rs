use crate::custom_event::RuffleEvent;
use crate::gui::text;
use crate::player::PlayerOptions;
use crate::util::pick_file;

use ruffle_core::backend::navigator::{OpenURLMode, SocketMode};
use ruffle_core::config::Letterbox;
use ruffle_core::{LoadBehavior, StageAlign, StageScaleMode};
use ruffle_render::quality::StageQuality;
use std::path::Path;
use unic_langid::LanguageIdentifier;
use url::Url;
use winit::event_loop::EventLoopProxy;

pub struct OpenDialog {
    options: PlayerOptions,
    event_loop: EventLoopProxy<RuffleEvent>,
    locale: LanguageIdentifier,

    // These are outside of PlayerOptions as it can be an invalid value (ie URL) during typing,
    // and we don't want to clear the value if the user, ie, toggles the checkbox.
    spoof_url: OptionalUrlField,
    base_url: OptionalUrlField,
    proxy_url: OptionalUrlField,
    path: PathOrUrlField,

    framerate: f64,
    framerate_enabled: bool,
}

impl OpenDialog {
    pub fn new(
        defaults: PlayerOptions,
        default_url: Option<Url>,
        event_loop: EventLoopProxy<RuffleEvent>,
        locale: LanguageIdentifier,
    ) -> Self {
        let spoof_url = OptionalUrlField::new(&defaults.spoof_url, "https://example.org/game.swf");
        let base_url = OptionalUrlField::new(&defaults.base, "https://example.org");
        let proxy_url = OptionalUrlField::new(&defaults.proxy, "socks5://localhost:8080");
        let path = PathOrUrlField::new(default_url, "path/to/movie.swf");
        Self {
            options: defaults,
            event_loop,
            locale,
            spoof_url,
            base_url,
            proxy_url,
            path,
            framerate: 30.0,
            framerate_enabled: false,
        }
    }

    fn start(&mut self) -> bool {
        if self.framerate_enabled {
            self.options.frame_rate = Some(self.framerate);
        } else {
            self.options.frame_rate = None;
        }
        if let Some(url) = self.path.value() {
            if self
                .event_loop
                .send_event(RuffleEvent::OpenURL(
                    url.clone(),
                    Box::new(self.options.clone()),
                ))
                .is_ok()
            {
                return true;
            }
        }

        false
    }

}

struct PathOrUrlField {
    value: String,
    result: Option<Url>,
    hint: &'static str,
}

impl PathOrUrlField {
    pub fn new(default: Option<Url>, hint: &'static str) -> Self {
        if let Some(default) = default {
            if default.scheme() == "file" {
                if let Ok(path) = default.to_file_path() {
                    return Self {
                        value: path.to_string_lossy().to_string(),
                        result: Some(default),
                        hint,
                    };
                }
            }

            return Self {
                value: default.to_string(),
                result: Some(default),
                hint,
            };
        }

        Self {
            value: "".to_string(),
            result: None,
            hint,
        }
    }

    pub fn value(&self) -> Option<&Url> {
        self.result.as_ref()
    }
}

struct OptionalUrlField {
    value: String,
    error: bool,
    enabled: bool,
    hint: &'static str,
}

impl OptionalUrlField {
    pub fn new(default: &Option<Url>, hint: &'static str) -> Self {
        if let Some(default) = default {
            Self {
                value: default.to_string(),
                error: false,
                enabled: true,
                hint,
            }
        } else {
            Self {
                value: "".to_string(),
                error: false,
                enabled: false,
                hint,
            }
        }
    }


    pub fn is_valid(&self) -> bool {
        !self.error
    }
}
