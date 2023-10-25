mod controller;
mod movie;
mod open_dialog;

pub use controller::GuiController;
pub use movie::MovieView;
use std::borrow::Cow;
use url::Url;

use crate::custom_event::RuffleEvent;
use crate::gui::open_dialog::OpenDialog;
use crate::player::PlayerOptions;
use chrono::DateTime;

use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{static_loader, Loader};
use rfd::FileDialog;
use ruffle_core::backend::ui::US_ENGLISH;
use ruffle_core::Player;
use std::collections::HashMap;
use std::fs;
use std::sync::MutexGuard;
use sys_locale::get_locale;
use unic_langid::LanguageIdentifier;
use winit::event_loop::EventLoopProxy;

const VERGEN_UNKNOWN: &str = "VERGEN_IDEMPOTENT_OUTPUT";

static_loader! {
    static TEXTS = {
        locales: "./assets/texts",
        fallback_language: "en-US"
    };
}

pub fn text<'a>(locale: &LanguageIdentifier, id: &'a str) -> Cow<'a, str> {
    TEXTS.lookup(locale, id).map(Cow::Owned).unwrap_or_else(|| {
        tracing::error!("Unknown desktop text id '{id}'");
        Cow::Borrowed(id)
    })
}

#[allow(dead_code)]
pub fn text_with_args<'a, T: AsRef<str>>(
    locale: &LanguageIdentifier,
    id: &'a str,
    args: &HashMap<T, FluentValue>,
) -> Cow<'a, str> {
    TEXTS
        .lookup_with_args(locale, id, args)
        .map(Cow::Owned)
        .unwrap_or_else(|| {
            tracing::error!("Unknown desktop text id '{id}'");
            Cow::Borrowed(id)
        })
}

/// Size of the top menu bar in pixels.
/// This is the offset at which the movie will be shown,
/// and added to the window size if trying to match a movie.
pub const MENU_HEIGHT: u32 = 24;

/// The main controller for the Ruffle GUI.
pub struct RuffleGui {
    event_loop: EventLoopProxy<RuffleEvent>,
    is_about_visible: bool,
    is_volume_visible: bool,
    volume_controls: VolumeControls,
    is_open_dialog_visible: bool,
    context_menu: Vec<ruffle_core::ContextMenuItem>,
    open_dialog: OpenDialog,
    locale: LanguageIdentifier,
    default_player_options: PlayerOptions,
    currently_opened: Option<(Url, PlayerOptions)>,
    was_suspended_before_debug: bool,
}

impl RuffleGui {
    fn new(
        event_loop: EventLoopProxy<RuffleEvent>,
        default_path: Option<Url>,
        default_player_options: PlayerOptions,
    ) -> Self {
        // TODO: language negotiation + https://github.com/1Password/sys-locale/issues/14
        // This should also be somewhere else so it can be supplied through UiBackend too

        let preferred_locale = get_locale();
        let locale = preferred_locale
            .and_then(|l| l.parse().ok())
            .unwrap_or_else(|| US_ENGLISH.clone());

        Self {
            is_about_visible: false,
            is_volume_visible: false,
            volume_controls: VolumeControls::new(false, default_player_options.volume * 100.0),
            is_open_dialog_visible: false,
            was_suspended_before_debug: false,

            context_menu: vec![],
            open_dialog: OpenDialog::new(
                default_player_options.clone(),
                default_path,
                event_loop.clone(),
                locale.clone(),
            ),

            event_loop,
            locale,
            default_player_options,
            currently_opened: None,
        }
    }

    pub fn show_context_menu(&mut self, menu: Vec<ruffle_core::ContextMenuItem>) {
        self.context_menu = menu;
    }

    pub fn is_context_menu_visible(&self) -> bool {
        !self.context_menu.is_empty()
    }

    /// Notifies the GUI that a new player was created.
    fn on_player_created(
        &mut self,
        opt: PlayerOptions,
        movie_url: Url,
        mut player: MutexGuard<Player>,
    ) {
        self.currently_opened = Some((movie_url.clone(), opt.clone()));

        // Update dialog state to reflect the newly-opened movie's options.
        self.is_open_dialog_visible = false;
        self.open_dialog = OpenDialog::new(
            opt,
            Some(movie_url),
            self.event_loop.clone(),
            self.locale.clone(),
        );

        player.set_volume(self.volume_controls.get_volume());
    }

    fn open_file_advanced(&mut self) {
        self.is_open_dialog_visible = true;
    }
}

/// The volume controls of the Ruffle GUI.
pub struct VolumeControls {
    is_muted: bool,
    volume: f32,
}

impl VolumeControls {
    fn new(is_muted: bool, volume: f32) -> Self {
        Self { is_muted, volume }
    }

    /// Returns the volume between 0 and 1 (calculated out of the
    /// checkbox and the slider).
    fn get_volume(&self) -> f32 {
        if !self.is_muted {
            self.volume / 100.0
        } else {
            0.0
        }
    }
}
