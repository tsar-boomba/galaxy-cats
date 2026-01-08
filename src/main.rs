//! Eat the cakes. Eat them all. An example 3D game.

pub mod game;
mod lobby;
mod lobby_config;

use std::net::SocketAddr;

use bevy::{prelude::*, window::WindowResolution};
use bevy_ggrs::{
    GgrsPlugin, GgrsSchedule, ReadInputs, RollbackApp, RollbackFrameRate, Session,
    ggrs::{DesyncDetection, GgrsEvent, PlayerType, SessionBuilder, UdpNonBlockingSocket},
};
use clap::Parser;

use crate::{lobby::LobbyPlugin, lobby_config::LobbyConfigPlugin};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
enum GameState {
    #[default]
    LobbyConfig,
    Lobby,
    Playing,
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
        .add_plugins(GgrsPlugin::<game::BoxConfig>::default())
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
        .add_plugins(LobbyConfigPlugin)
        .add_plugins(LobbyPlugin)
        // define frequency of rollback game logic update
        .insert_resource(RollbackFrameRate(FPS))
        // this system will be executed as part of input reading
        .add_systems(ReadInputs, game::read_local_inputs)
        // Rollback behavior can be customized using a variety of extension methods and plugins:
        // The FrameCount resource implements Copy, we can use that to have minimal overhead rollback
        .rollback_resource_with_copy::<game::FrameCount>()
        // Same with the Velocity Component
        .rollback_component_with_copy::<game::Velocity>()
        // Transform only implements Clone, so instead we'll use that to snapshot and rollback with
        .rollback_component_with_clone::<Transform>()
        .rollback_component_with_copy::<game::TrailSegment>()
        // register a resource that will be rolled back
        .insert_resource(game::FrameCount { frame: 0 })
        // print some network stats - not part of the rollback schedule as it does not need to be rolled back
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, setup_cameras)
        .add_systems(OnEnter(GameState::Playing), game::setup)
        // these systems will be executed as part of the advance frame update
        .add_systems(
            GgrsSchedule,
            (
                (
                    game::move_player,
                    game::move_camera,
                    game::manage_trail,
                    game::check_collisions,
                )
                    .chain(),
                game::increase_frame_system,
            ),
        )
        .add_systems(Update, print_network_stats_system)
        .add_systems(Update, print_events_system)
        .run();

    Ok(())
}

fn print_events_system(mut session: Option<ResMut<Session<game::BoxConfig>>>) {
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
    p2p_session: Option<Res<Session<game::BoxConfig>>>,
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
