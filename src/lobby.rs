use bevy::prelude::*;
use bevy_ggrs::{ggrs::DesyncDetection, prelude::*};
use bevy_matchbox::prelude::*;

use crate::{FPS, GameState, game, lobby_config::LobbyConfig};

pub struct LobbyPlugin;

#[derive(Default, Clone, Copy, Component)]
struct LobbyEntity;

#[derive(Default, Clone, Copy, Component)]
struct MainText;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Lobby), lobby_setup)
            .add_systems(OnExit(GameState::Lobby), lobby_cleanup)
            .add_systems(Update, lobby_system.run_if(in_state(GameState::Lobby)));
    }
}

fn lobby_setup(mut commands: Commands) {
    // All this is just for spawning centered text.
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexEnd,
                ..default()
            },
            BackgroundColor(Color::srgb(0.43, 0.41, 0.38)),
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        align_self: AlignSelf::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    Text("Entering lobby...".to_string()),
                    TextFont {
                        font_size: 96.,
                        ..default()
                    },
                    TextColor(Color::BLACK),
                ))
                .insert(MainText);
        })
        .insert(LobbyEntity);
}

fn lobby_system(
    mut app_state: ResMut<NextState<GameState>>,
    config: Res<LobbyConfig>,
    mut socket: ResMut<MatchboxSocket>,
    mut commands: Commands,
    mut text: Single<&mut Text, With<MainText>>,
    existing_session: Option<ResMut<Session<game::BoxConfig>>>,
) {
    // regularly call update_peers to update the list of connected peers
    let Ok(peer_changes) = socket.try_update_peers() else {
        warn!("socket dropped");
        app_state.set(GameState::LobbyConfig);
        return;
    };

    for (peer, new_state) in peer_changes {
        // you can also handle the specific dis(connections) as they occur:
        match new_state {
            PeerState::Connected => info!("peer {peer} connected"),
            PeerState::Disconnected => info!("peer {peer} disconnected"),
        }
    }

    let connected_peers = socket.connected_peers().count();
    let remaining = config.players - (connected_peers + 1);
    text.0 = format!("Waiting for {remaining} more player(s)",);
    if remaining > 0 {
        return;
    }

    info!("All peers have joined, going in-game");
    if existing_session.is_some() {
        // transition to in-game state
        app_state.set(GameState::Playing);
        return;
    }

    // extract final player list
    let players = socket.players();

    // create a GGRS P2P session
    let mut sess_build = SessionBuilder::<game::BoxConfig>::new()
        .with_num_players(config.players)
        .with_max_prediction_window(12)
        .with_input_delay(2)
        .with_desync_detection_mode(DesyncDetection::On {
            interval: FPS as u32,
        });

    for (i, player) in players.into_iter().enumerate() {
        sess_build = sess_build
            .add_player(player, i)
            .expect("failed to add player");
    }

    let channel = socket.take_channel(0).unwrap();

    // start the GGRS session
    let sess = sess_build
        .start_p2p_session(channel)
        .expect("failed to start session");

    commands.insert_resource(Session::P2P(sess));

    // transition to in-game state
    app_state.set(GameState::Playing);
}

fn lobby_cleanup(mut commands: Commands, entities: Query<Entity, With<LobbyEntity>>) {
    for entity in entities {
        commands.entity(entity).despawn();
    }
}
