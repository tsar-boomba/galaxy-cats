//! Eat the cakes. Eat them all. An example 3D game.

mod game;

use std::net::SocketAddr;

use bevy::{prelude::*, window::WindowResolution};
use bevy_ggrs::{
    GgrsPlugin, GgrsSchedule, ReadInputs, RollbackApp, RollbackFrameRate, Session,
    ggrs::{DesyncDetection, GgrsEvent, PlayerType, SessionBuilder, UdpNonBlockingSocket},
};
use clap::Parser;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
enum GameState {
    #[default]
    Playing,
    GameOver,
}

const FPS: usize = 60;

// clap will read command line arguments
#[derive(Parser, Resource)]
struct Opt {
    #[clap(short, long)]
    local_port: u16,
    #[clap(long, num_args = 0..3)]
    id: usize,
    #[clap(short, long, num_args = 1..)]
    players: Vec<String>,
    #[clap(short, long, num_args = 1..)]
    spectators: Vec<SocketAddr>,
}

#[derive(Resource)]
struct NetworkStatsTimer(Timer);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // read cmd line arguments
    let opt = Opt::parse();
    let num_players = opt.players.len();
    assert!(num_players > 0);

    // create a GGRS session
    let mut sess_build = SessionBuilder::<game::BoxConfig>::new()
        .with_num_players(num_players)
        .with_desync_detection_mode(DesyncDetection::On {
            interval: FPS as u32,
        }); // (optional) set how often to exchange state checksums

    // add players
    for (i, player_addr) in opt.players.iter().enumerate() {
        // local player
        if player_addr == "localhost" {
            sess_build = sess_build.add_player(PlayerType::Local, i)?;
        } else {
            // remote players
            let remote_addr: SocketAddr = player_addr.parse()?;
            sess_build = sess_build.add_player(PlayerType::Remote(remote_addr), i)?;
        }
    }

    // optionally, add spectators
    for (i, spec_addr) in opt.spectators.iter().enumerate() {
        sess_build = sess_build.add_player(PlayerType::Spectator(*spec_addr), num_players + i)?;
    }

    // start the GGRS session
    let socket = UdpNonBlockingSocket::bind_to_port(opt.local_port)?;
    let sess = sess_build.start_p2p_session(socket)?;

    App::new()
        .add_plugins(GgrsPlugin::<game::BoxConfig>::default())
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
        .insert_resource(opt)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(640, 640),
                title: "Galaxy Cats".to_owned(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        // add your GGRS session
        .insert_resource(Session::P2P(sess))
        // register a resource that will be rolled back
        .insert_resource(game::FrameCount { frame: 0 })
        // print some network stats - not part of the rollback schedule as it does not need to be rolled back
        .insert_resource(NetworkStatsTimer(Timer::from_seconds(
            2.0,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, (game::setup, setup_cameras))
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

fn print_events_system(mut session: ResMut<Session<game::BoxConfig>>) {
    match session.as_mut() {
        Session::P2P(s) => {
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
        _ => panic!("This example focuses on p2p."),
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
