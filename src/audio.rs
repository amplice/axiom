use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::events::GameEventBus;

const MAX_AUDIO_EVENTS: usize = 256;

fn default_volume() -> f32 {
    1.0
}

fn default_looping() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SfxDefinition {
    pub path: String,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default)]
    pub pitch_variance: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MusicDefinition {
    pub path: String,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default = "default_looping")]
    pub looping: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AudioEventLog {
    pub frame: u64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_event: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AudioStateSnapshot {
    pub current_music: Option<String>,
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub sfx: HashMap<String, SfxDefinition>,
    pub music: HashMap<String, MusicDefinition>,
    pub triggers: HashMap<String, String>,
    pub recent_events: Vec<AudioEventLog>,
}

#[derive(Resource)]
pub struct AudioManager {
    pub sfx: HashMap<String, SfxDefinition>,
    pub music: HashMap<String, MusicDefinition>,
    pub triggers: HashMap<String, String>,
    pub current_music: Option<String>,
    pub master_volume: f32,
    pub sfx_volume: f32,
    pub music_volume: f32,
    pub recent_events: Vec<AudioEventLog>,
}

impl Default for AudioManager {
    fn default() -> Self {
        Self {
            sfx: HashMap::new(),
            music: HashMap::new(),
            triggers: HashMap::new(),
            current_music: None,
            master_volume: 1.0,
            sfx_volume: 1.0,
            music_volume: 1.0,
            recent_events: Vec::new(),
        }
    }
}

impl AudioManager {
    pub fn snapshot(&self) -> AudioStateSnapshot {
        AudioStateSnapshot {
            current_music: self.current_music.clone(),
            master_volume: self.master_volume,
            sfx_volume: self.sfx_volume,
            music_volume: self.music_volume,
            sfx: self.sfx.clone(),
            music: self.music.clone(),
            triggers: self.triggers.clone(),
            recent_events: self.recent_events.clone(),
        }
    }

    pub fn set_sfx(&mut self, effects: HashMap<String, SfxDefinition>) {
        self.sfx = effects;
    }

    pub fn set_music(&mut self, tracks: HashMap<String, MusicDefinition>) {
        self.music = tracks;
        if let Some(current) = self.current_music.as_ref() {
            if !self.music.contains_key(current) {
                self.current_music = None;
            }
        }
    }

    pub fn set_triggers(&mut self, mappings: HashMap<String, String>) {
        self.triggers = mappings;
    }

    pub fn set_volume(&mut self, channel: &str, value: f32, frame: u64) -> Result<(), String> {
        let v = value.clamp(0.0, 2.0);
        match channel {
            "master" => self.master_volume = v,
            "sfx" => self.sfx_volume = v,
            "music" => self.music_volume = v,
            _ => return Err(format!("Unknown volume channel: {channel}")),
        }
        self.push_event(AudioEventLog {
            frame,
            event_type: "volume".to_string(),
            name: channel.to_string(),
            action: Some("set".to_string()),
            volume: Some(v),
            pitch: None,
            source_event: None,
        });
        Ok(())
    }

    pub fn play_sfx(
        &mut self,
        name: &str,
        frame: u64,
        volume_scale: Option<f32>,
        pitch: Option<f32>,
        source_event: Option<String>,
    ) -> Result<(), String> {
        let Some(def) = self.sfx.get(name) else {
            return Err(format!("Unknown sfx: {name}"));
        };
        let volume =
            def.volume * self.sfx_volume * self.master_volume * volume_scale.unwrap_or(1.0);
        self.push_event(AudioEventLog {
            frame,
            event_type: "sfx".to_string(),
            name: name.to_string(),
            action: Some("play".to_string()),
            volume: Some(volume),
            pitch,
            source_event,
        });
        Ok(())
    }

    pub fn play_music(
        &mut self,
        name: &str,
        frame: u64,
        _fade_in: Option<f32>,
        source_event: Option<String>,
    ) -> Result<(), String> {
        let Some(def) = self.music.get(name) else {
            return Err(format!("Unknown music track: {name}"));
        };
        let volume = def.volume * self.music_volume * self.master_volume;
        self.current_music = Some(name.to_string());
        self.push_event(AudioEventLog {
            frame,
            event_type: "music".to_string(),
            name: name.to_string(),
            action: Some("start".to_string()),
            volume: Some(volume),
            pitch: None,
            source_event,
        });
        Ok(())
    }

    pub fn stop_music(&mut self, frame: u64, _fade_out: Option<f32>) {
        if let Some(name) = self.current_music.take() {
            self.push_event(AudioEventLog {
                frame,
                event_type: "music".to_string(),
                name,
                action: Some("stop".to_string()),
                volume: None,
                pitch: None,
                source_event: None,
            });
        }
    }

    fn push_event(&mut self, event: AudioEventLog) {
        self.recent_events.push(event);
        if self.recent_events.len() > MAX_AUDIO_EVENTS {
            let excess = self.recent_events.len() - MAX_AUDIO_EVENTS;
            self.recent_events.drain(0..excess);
        }
    }
}

#[derive(Resource, Default)]
struct AudioEventCursor {
    last_frame: u64,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AudioManager::default())
            .insert_resource(AudioEventCursor::default())
            .add_systems(Update, auto_audio_from_events);
    }
}

fn auto_audio_from_events(
    mut audio: ResMut<AudioManager>,
    bus: Res<GameEventBus>,
    mut cursor: ResMut<AudioEventCursor>,
) {
    let mut newest_frame = cursor.last_frame;
    for ev in bus.recent.iter().filter(|ev| ev.frame > cursor.last_frame) {
        newest_frame = newest_frame.max(ev.frame);
        handle_event(&mut audio, ev);
    }
    cursor.last_frame = newest_frame;
}

fn handle_event(audio: &mut AudioManager, ev: &crate::events::GameEvent) {
    match ev.name.as_str() {
        "audio_play_sfx" => {
            let name = ev
                .data
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty());
            if let Some(name) = name {
                let volume = ev
                    .data
                    .get("volume")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32);
                let pitch = ev
                    .data
                    .get("pitch")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32);
                let _ = audio.play_sfx(name, ev.frame, volume, pitch, None);
            }
        }
        "audio_play_music" => {
            let name = ev
                .data
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty());
            if let Some(name) = name {
                let fade_in = ev
                    .data
                    .get("fade_in")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32);
                let _ = audio.play_music(name, ev.frame, fade_in, None);
            }
        }
        "audio_stop_music" => {
            let fade_out = ev
                .data
                .get("fade_out")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            audio.stop_music(ev.frame, fade_out);
        }
        "audio_set_volume" => {
            let channel = ev
                .data
                .get("channel")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or("");
            let value = ev
                .data
                .get("value")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);
            if !channel.is_empty() {
                if let Some(value) = value {
                    let _ = audio.set_volume(channel, value, ev.frame);
                }
            }
        }
        other => {
            if let Some(mapped) = audio.triggers.get(other).cloned() {
                let _ = audio.play_sfx(&mapped, ev.frame, None, None, Some(other.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_sfx_records_event() {
        let mut audio = AudioManager::default();
        audio.sfx.insert(
            "jump".to_string(),
            SfxDefinition {
                path: "audio/jump.ogg".to_string(),
                volume: 0.5,
                pitch_variance: 0.0,
            },
        );
        audio.master_volume = 0.8;
        audio.sfx_volume = 0.5;

        audio
            .play_sfx("jump", 10, Some(1.0), Some(1.2), Some("test".to_string()))
            .expect("sfx should play");

        assert_eq!(audio.recent_events.len(), 1);
        let ev = &audio.recent_events[0];
        assert_eq!(ev.event_type, "sfx");
        assert_eq!(ev.name, "jump");
        assert_eq!(ev.frame, 10);
        assert_eq!(ev.volume, Some(0.2));
        assert_eq!(ev.pitch, Some(1.2));
    }

    #[test]
    fn invalid_channel_rejected() {
        let mut audio = AudioManager::default();
        let err = audio
            .set_volume("invalid", 1.0, 0)
            .expect_err("invalid channel should fail");
        assert!(err.contains("Unknown volume channel"));
    }
}
