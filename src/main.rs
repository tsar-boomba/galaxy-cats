//! Eat the cakes. Eat them all. An example 3D game.

#![feature(iter_intersperse)]

pub mod game;
mod lobby;
mod lobby_config;

use bevy::{prelude::*, window::WindowResolution};
use bevy_ggrs::{Session, ggrs::GgrsEvent};

use crate::{game::GamePlugin, lobby::LobbyPlugin, lobby_config::LobbyConfigPlugin};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
pub enum GameState {
    #[default]
    LobbyConfig,
    Lobby,
    Playing,
    GameEnd,
}

// On non-web and web with WebGPU, target 60 FPS
#[cfg(any(not(target_arch = "wasm32"), feature = "webgpu"))]
const FPS: usize = 60;
// On WebGL target 30 FPS
#[cfg(all(target_arch = "wasm32", not(feature = "webgpu")))]
const FPS: usize = 30;

#[derive(Resource)]
struct NetworkStatsTimer(Timer);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(640, 640),
                title: "Galaxy Cats".to_owned(),
                // fill the entire browser window
                fit_canvas_to_parent: true,
                // don't hijack keyboard shortcuts like F5, F6, F12, Ctrl+R etc.
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_plugins((LobbyConfigPlugin, LobbyPlugin, GamePlugin))
        // print some network stats - not part of the rollback schedule as it does not need to be rolled back
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, setup_cameras)
        .add_systems(Update, (print_network_stats_system, print_events_system))
        .run();

    Ok(())
}

fn print_events_system(mut session: Option<ResMut<Session<game::GameConfig>>>) {
    match session.as_deref_mut() {
        Some(Session::P2P(s)) => {
            for event in s.events() {
                match event {
                    GgrsEvent::Disconnected { .. } | GgrsEvent::NetworkInterrupted { .. } => {
                        log::warn!("GGRS event: {event:?}")
                    }
                    GgrsEvent::DesyncDetected { .. } => log::error!("GGRS event: {event:?}"),
                    _ => log::info!("GGRS event: {event:?}"),
                }
            }
        }
        _ => {
            // No P2P session yet
        }
    }
}

fn print_network_stats_system(
    time: Res<Time>,
    mut timer: ResMut<NetworkStatsTimer>,
    p2p_session: Option<Res<Session<game::GameConfig>>>,
) {
    // print only when timer runs out
    if timer.0.tick(time.delta()).just_finished()
        && let Some(sess) = p2p_session
    {
        match sess.as_ref() {
            Session::P2P(s) => {
                let num_players = s.num_players();
                for i in 0..num_players {
                    if let Ok(stats) = s.network_stats(i) {
                        log::info!("NetworkStats for player {}: {:?}", i, stats);
                    }
                }
            }
            _ => panic!("This examples focuses on p2p."),
        }
    }
}

fn setup_cameras(mut commands: Commands) {
    commands.spawn((Camera3d::default(), Transform::default()));
}
