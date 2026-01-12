use bevy::prelude::*;
use bevy_ggrs::Session;
use bevy_matchbox::{
    MatchboxSocket,
    matchbox_socket::{RtcIceServerConfig, WebRtcSocket},
};

use crate::{GameState, game};

#[derive(Resource, Default)]
pub struct LobbyConfig {
    pub players: usize,
    pub server: String,
    pub room: String,
}

pub struct LobbyConfigPlugin;

#[derive(Component)]
struct ConfigLobbyEntity;

#[derive(Component)]
enum ButtonType {
    TwoPlayers,
    ThreePlayers,
    FourPlayers,
    FivePlayers,
    SixPlayers,
    Join,
}

const MIN_PLAYERS: usize = 2;
const MAX_PLAYERS: usize = 6;

impl Plugin for LobbyConfigPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LobbyConfig>()
            .add_systems(OnEnter(GameState::LobbyConfig), lobby_config_setup)
            .add_systems(OnExit(GameState::LobbyConfig), lobby_config_cleanup)
            .add_systems(
                Update,
                lobby_config_system.run_if(in_state(GameState::LobbyConfig)),
            );
    }
}

fn lobby_config_setup(
    mut commands: Commands,
    mut lobby_config: ResMut<LobbyConfig>,
    _asset_server: Res<AssetServer>,
    old_socket: Option<ResMut<MatchboxSocket>>,
) {
    *lobby_config = LobbyConfig::default();

    // Reset networking stuff when entering lobby_config
    if let Some(mut old_socket) = old_socket {
        old_socket.close();
        commands.remove_resource::<MatchboxSocket>();
    }

    commands.remove_resource::<Session<game::GameConfig>>();

    // All this is just for spawning centered text.
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgb(0.43, 0.41, 0.38)),
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    align_self: AlignSelf::Center,
                    justify_content: JustifyContent::Center,
                    ..Default::default()
                },
                Text("Config Lobby...".to_string()),
                TextFont {
                    font_size: 96.,
                    ..default()
                },
                TextColor(Color::BLACK),
            ));
            parent.spawn((
                Node {
                    align_self: AlignSelf::Center,
                    justify_content: JustifyContent::Center,
                    ..Default::default()
                },
                Text::new("# of Players"),
                TextFont {
                    font_size: 96.,
                    ..default()
                },
                TextColor(Color::BLACK),
            ));
            parent.spawn((
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                children![
                    button("2", ButtonType::TwoPlayers),
                    button("3", ButtonType::ThreePlayers),
                    button("4", ButtonType::FourPlayers),
                    button("5", ButtonType::FivePlayers),
                    button("6", ButtonType::SixPlayers),
                ],
            ));

            parent.spawn(button("Join!", ButtonType::Join));
        })
        .insert(ConfigLobbyEntity);
}

fn lobby_config_system(
    mut commands: Commands,
    mut app_state: ResMut<NextState<GameState>>,
    mut lobby_config: ResMut<LobbyConfig>,
    mut interaction_query: Query<
        (Entity, &Interaction, &mut Button, &ButtonType),
        Changed<Interaction>,
    >,
) {
    for (_entity, interaction, mut _button, button_type) in &mut interaction_query {
        match *interaction {
            Interaction::Pressed => {
                match button_type {
                    ButtonType::TwoPlayers => {
                        lobby_config.players = 2;
                    }
                    ButtonType::ThreePlayers => {
                        lobby_config.players = 3;
                    }
                    ButtonType::FourPlayers => {
                        lobby_config.players = 4;
                    }
                    ButtonType::FivePlayers => {
                        lobby_config.players = 5;
                    }
                    ButtonType::SixPlayers => {
                        lobby_config.players = 6;
                    }
                    ButtonType::Join => {
                        // TODO: actually input server/room
                        #[cfg(not(debug_assertions))]
                        {
                            lobby_config.server = "wss://gc-matchbox.igamble.dev".into();
                        }
                        #[cfg(debug_assertions)]
                        {
                            lobby_config.server = "ws://localhost:3536".into();
                        }

                        lobby_config.room = "bevy_ggrs".into();
                        if (MIN_PLAYERS..=MAX_PLAYERS).contains(&lobby_config.players)
                            && !lobby_config.server.is_empty()
                            && !lobby_config.room.is_empty()
                        {
                            // connect and transition to lobby state
                            let room_url = format!(
                                "{}/{}?next={}",
                                lobby_config.server, lobby_config.room, lobby_config.players
                            );
                            info!("connecting to matchbox server: {room_url:?}");

                            commands.insert_resource(MatchboxSocket::from(
                                WebRtcSocket::builder(room_url)
                                    .add_unreliable_channel()
                                    .ice_server(RtcIceServerConfig {
                                        urls: vec![
                                            "stun:stun.l.google.com:19302".to_string(),
                                            "stun:stun1.l.google.com:19302".to_string(),
                                            "turn:gc-server.igamble.dev:3478".to_string(),
                                            "turn:gc-server.igamble.dev:3478?transport=tcp".to_string(),
                                        ],
                                        // TODO: real turn auth???
                                        username: Some("username".into()),
                                        credential: Some("password".into()),
                                    })
                                    .build(),
                            ));
                            app_state.set(GameState::Lobby);
                            return;
                        }
                    }
                }
            }
            Interaction::Hovered => {}
            Interaction::None => {}
        }
    }
}

fn lobby_config_cleanup(mut commands: Commands, entities: Query<Entity, With<ConfigLobbyEntity>>) {
    for entity in entities {
        commands.entity(entity).despawn();
    }
}

fn button(text: impl Into<String>, extra_bundle: impl Bundle) -> impl Bundle {
    (
        Button,
        Node {
            width: px(150),
            height: px(65),
            border: UiRect::all(px(2)),
            // horizontally center child text
            justify_content: JustifyContent::Center,
            // vertically center child text
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(px(8)),
            ..Default::default()
        },
        BorderColor::all(Color::WHITE),
        BackgroundColor(Color::BLACK),
        extra_bundle,
        children![(
            Text::new(text),
            TextFont {
                font_size: 33.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
        )],
    )
}
