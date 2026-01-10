use std::{f32::consts::PI, time::Duration};

use bevy::{platform::collections::HashMap, prelude::*};
use bevy_ggrs::{LocalInputs, LocalPlayers, prelude::*};
use bevy_matchbox::prelude::*;
use bevy_roll_safe::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{FPS, GameState};

const INPUT_JUMP: u8 = 1 << 0;
const INPUT_LEFT: u8 = 1 << 1;
const INPUT_RIGHT: u8 = 1 << 2;
const INPUT_DASH: u8 = 1 << 3;

const SPHERE_RADIUS: f32 = 4.0;
const SPHERE_RADIUS_SQ: f32 = SPHERE_RADIUS * SPHERE_RADIUS;
const MOVE_SPEED: f32 = 5.0;
const ROTATE_SPEED: f32 = 0.5;
const GRAVITY: f32 = -75.0;
const JUMP_VELOCITY: f32 = 15.0;
const FUEL_USAGE: f32 = 50.0;
const FUEL_REGEN: f32 = 10.0;
const DASH_SPEED_MULTIPLIER: f32 = 2.0;
const DASH_LENGTH: f32 = 0.7;
const DASH_COOLDOWN: f32 = 4.0;
const PLAYER_RADIUS: f32 = 0.2;
const TRAIL_RADIUS: f32 = 0.2;
const TRAIL_SPAWN_DIST: f32 = TRAIL_RADIUS / 2.0;
/// Trail must exist for this many seconds before it kills people
const MIN_TRAIL_LIFE: f64 = 0.25;
const PLAYER_COLOR: [Color; 4] = [
    Color::srgb(1.0, 0.0, 0.0),
    Color::srgb(0.0, 0.0, 1.0),
    Color::srgb(0.0, 1.0, 0.0),
    Color::srgb(1.0, 0.5, 0.0),
];

// You need to define a config struct to bundle all the generics of GGRS. bevy_ggrs provides a sensible default in `GgrsConfig`.
// (optional) You can define a type here for brevity.
pub type BoxConfig = GgrsConfig<Input, PeerId>;

#[derive(States, Clone, Eq, PartialEq, Debug, Hash, Default, Reflect)]
enum RollbackState {
    #[default]
    None,
    /// When the characters running around
    InRound,
    /// When one character is left, and we're transitioning to the next round
    RoundEnd,
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Input(u8);

#[derive(Default, Component, Clone)]
pub struct Player {
    pub handle: usize,
    pub fuel: f32,
    pub hovering: bool,
    pub dashing: Timer,
    pub dash_cooldown: Timer,
    pub last_trail_pos: Vec3,
    pub last_trail: Option<Entity>,
}

// Components that should be saved/loaded need to support snapshotting. The built-in options are:
// - Clone (Recommended)
// - Copy
// - Reflect
// See `bevy_ggrs::Strategy` for custom alternatives
#[derive(Default, Reflect, Component, Clone, Copy, Deref, DerefMut)]
pub struct Velocity(pub Vec3);

#[derive(Default, Clone, Copy, Component)]
pub struct TrailSegment {
    created_at: f64,
}

// You can also register resources.
#[derive(Resource, Default, Reflect, Hash, Clone, Copy)]
#[reflect(Hash)]
pub struct FrameCount {
    pub frame: u32,
}

#[derive(Resource, Clone, Deref, DerefMut)]
struct RoundEndTimer(Timer);

impl Default for RoundEndTimer {
    fn default() -> Self {
        RoundEndTimer(Timer::from_seconds(0.75, TimerMode::Repeating))
    }
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            GgrsPlugin::<BoxConfig>::default(),
            RollbackSchedulePlugin::new_ggrs(),
        ))
        .init_ggrs_state::<RollbackState>()
        // define frequency of rollback game logic update
        .insert_resource(RollbackFrameRate(FPS))
        .init_resource::<RoundEndTimer>()
        // this system will be executed as part of input reading
        .add_systems(ReadInputs, read_local_inputs)
        // Rollback behavior can be customized using a variety of extension methods and plugins:
        // The FrameCount resource implements Copy, we can use that to have minimal overhead rollback
        .rollback_resource_with_copy::<FrameCount>()
        // Same with the Velocity Component
        .rollback_component_with_copy::<Velocity>()
        // Transform only implements Clone, so instead we'll use that to snapshot and rollback with
        .rollback_component_with_clone::<Transform>()
        .rollback_component_with_copy::<TrailSegment>()
        .rollback_component_with_clone::<Player>()
        .rollback_component_with_clone::<SceneRoot>()
        .rollback_resource_with_clone::<RoundEndTimer>()
        // register a resource that will be rolled back
        .insert_resource(FrameCount { frame: 0 })
        .add_systems(OnEnter(GameState::Playing), setup_env)
        .add_systems(OnEnter(RollbackState::InRound), spawn_players)
        // these systems will be executed as part of the advance frame update
        .add_systems(
            RollbackUpdate,
            (
                move_player,
                manage_trail.after(move_player),
                move_camera.after(manage_trail),
                check_collisions.after(move_camera),
            )
                .run_if(in_state(RollbackState::InRound))
                .after(bevy_roll_safe::apply_state_transition::<RollbackState>),
        )
        .add_systems(
            RollbackUpdate,
            round_end_timeout
                .ambiguous_with(check_collisions)
                .run_if(in_state(RollbackState::RoundEnd)),
        );
    }
}

/// Collects player inputs during [`ReadInputs`](`bevy_ggrs::ReadInputs`) and creates a [`LocalInputs`] resource.
pub fn read_local_inputs(
    mut commands: Commands,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    local_players: Res<LocalPlayers>,
) {
    let mut local_inputs = HashMap::new();

    for handle in &local_players.0 {
        let mut input: u8 = 0;

        if keyboard_input.pressed(KeyCode::ArrowLeft) {
            input |= INPUT_LEFT;
        }
        if keyboard_input.pressed(KeyCode::ArrowRight) {
            input |= INPUT_RIGHT;
        }
        if keyboard_input.pressed(KeyCode::Space) {
            input |= INPUT_JUMP;
        }
        if keyboard_input.pressed(KeyCode::KeyZ) {
            input |= INPUT_DASH;
        }

        local_inputs.insert(*handle, Input(input));
    }

    commands.insert_resource(LocalInputs::<BoxConfig>(local_inputs));
}

/// Setup sphere and lights then set rollback state to in round
fn setup_env(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut next_state: ResMut<NextState<RollbackState>>,
) {
    // Light
    commands.spawn((
        DespawnOnExit(GameState::Playing),
        PointLight {
            intensity: 2_000_000.0,
            shadows_enabled: true,
            range: 30.0,
            ..default()
        },
        Transform::from_xyz(0.0, SPHERE_RADIUS + 10.0, 0.0),
    ));

    commands.spawn((
        DespawnOnExit(GameState::Playing),
        PointLight {
            intensity: 2_000_000.0,
            shadows_enabled: true,
            range: 30.0,
            ..default()
        },
        Transform::from_xyz(0.0, -SPHERE_RADIUS - 10.0, 0.0),
    ));

    // Sphere
    commands.spawn((
        DespawnOnExit(GameState::Playing),
        Mesh3d(meshes.add(Sphere::new(SPHERE_RADIUS))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba_u8(64, 198, 255, 104),
            alpha_mode: AlphaMode::Blend,
            ..Default::default()
        })),
        Transform {
            translation: Vec3::ZERO,
            ..Default::default()
        },
    ));

    next_state.set(RollbackState::InRound);
}

/// make sure no leftover players or trails, then spawn in players
fn spawn_players(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    session: Res<Session<BoxConfig>>,
    players: Query<Entity, With<Player>>,
    trails: Query<Entity, With<TrailSegment>>,
) {
    for player in players {
        commands.entity(player).despawn();
    }

    for trail in trails {
        commands.entity(trail).despawn();
    }

    let num_players = match &*session {
        Session::SyncTest(s) => s.num_players(),
        Session::P2P(s) => s.num_players(),
        Session::Spectator(s) => s.num_players(),
    };

    for handle in 0..num_players {
        // Entities which will be rolled back can be created just like any other...
        let mut dashing = Timer::from_seconds(DASH_LENGTH, TimerMode::Once);
        dashing.finish();

        let mut dash_cooldown = Timer::from_seconds(DASH_COOLDOWN, TimerMode::Once);
        dash_cooldown.finish();

        // TODO: add some way for each client to know which player is which
        let spawn_pos = match handle {
            0 => Vec3::new(0., SPHERE_RADIUS, 0.),
            1 => Vec3::new(0., -SPHERE_RADIUS, 0.),
            4 => Vec3::new(0., 0., SPHERE_RADIUS),
            5 => Vec3::new(0., 0., -SPHERE_RADIUS),
            2 => Vec3::new(SPHERE_RADIUS, 0., 0.),
            3 => Vec3::new(-SPHERE_RADIUS, 0., 0.),
            _ => panic!("Too many players!"),
        };

        let spawn_rot = match handle {
            0 => Quat::from_rotation_y(-PI / 2.),
            1 => Quat::from_rotation_y(PI / 2.),
            4 => Quat::from_rotation_z(-PI / 2.),
            5 => Quat::from_rotation_z(PI / 2.),
            2 => Quat::from_rotation_x(-PI / 2.),
            3 => Quat::from_rotation_x(PI / 2.),
            _ => panic!("Too many players!"),
        };

        commands
            .spawn((
                DespawnOnExit(GameState::Playing),
                Transform {
                    translation: spawn_pos,
                    rotation: spawn_rot,
                    ..default()
                },
                Player {
                    handle,
                    fuel: 100.0,
                    hovering: false,
                    dashing,
                    dash_cooldown,
                    last_trail_pos: spawn_pos,
                    last_trail: None,
                },
                Velocity::default(),
                SceneRoot(
                    asset_server
                        .load(GltfAssetLabel::Scene(0).from_asset("models/AlienCake/alien.glb")),
                ),
            ))
            .add_rollback();
    }
}

// Example system, manipulating a resource, will be added to the rollback schedule.
// Increases the frame count by 1 every update step. If loading and saving resources works correctly,
// you should see this resource rolling back, counting back up and finally increasing by 1 every update step
#[allow(dead_code)]
pub fn increase_frame_system(mut frame_count: ResMut<FrameCount>) {
    frame_count.frame += 1;
}

pub fn move_player(
    query: Query<(&mut Transform, &mut Velocity, &mut Player), With<Player>>,
    inputs: Res<PlayerInputs<BoxConfig>>,
    // Thanks to RollbackTimePlugin, this is rollback safe
    time: Res<Time>,
) {
    let dt = time.delta_secs();

    for (mut transform, mut vel, mut player) in query {
        let inputs = inputs[player.handle].0.0;
        let left = inputs & INPUT_LEFT != 0;
        let right = inputs & INPUT_RIGHT != 0;
        let jump = inputs & INPUT_JUMP != 0;
        let dash = inputs & INPUT_DASH != 0;

        // We turn around the local Y axis (the alien's "up")
        if left {
            transform.rotate_local_y(PI * ROTATE_SPEED * dt);
        }

        if right {
            transform.rotate_local_y(-PI * ROTATE_SPEED * dt);
        }

        let is_grounded = transform.translation.length_squared() <= SPHERE_RADIUS_SQ + 0.02;

        // Start dashing if dash was pressed
        player.dash_cooldown.tick(Duration::from_secs_f32(dt));
        if dash && player.dashing.is_finished() && player.dash_cooldown.is_finished() && is_grounded
        {
            player.dashing.reset();
            player.dash_cooldown.reset();
        }
        player.dashing.tick(Duration::from_secs_f32(dt));

        if jump && is_grounded {
            vel.y = JUMP_VELOCITY;

            // Jumping ends dash and immediately makes it available again
            player.dashing.finish();
            player.dash_cooldown.finish();
        }

        let delta_grav = GRAVITY * dt;
        // Would start to fall on this update, if jump is held, start hovering
        if jump
            && vel.y.is_sign_positive()
            && (vel.y + delta_grav).is_sign_negative()
            && !is_grounded
            && !player.hovering
        {
            player.hovering = true;
        }

        // Decrement fuel while hovering
        if player.hovering && jump {
            player.fuel -= FUEL_USAGE * dt;
        }

        // Stop hover if jump is released or out of fuel
        if player.hovering && (!jump || player.fuel <= 0.0) {
            player.hovering = false;
            player.fuel = 0.0;
        }

        // Apply Gravity if in air and not hovering
        if !player.hovering && (!is_grounded || vel.y != 0.0) {
            vel.y += delta_grav;
        } else {
            vel.y = 0.0;
        }

        // The position vector IS the "up" vector since the sphere is centered at (0,0,0)
        let current_pos = transform.translation;
        let forward = transform.forward().as_vec3();

        // THE MATH:
        // To move forward on a sphere, we rotate the POSITION vector
        // around an axis that is perpendicular to both UP and FORWARD.
        let axis = transform.right().as_vec3(); // This is the "side-to-side" axis
        let move_speed = if player.dashing.is_finished() {
            MOVE_SPEED
        } else {
            DASH_SPEED_MULTIPLIER * MOVE_SPEED
        };
        let move_amount = move_speed * dt;
        let angle = move_amount / SPHERE_RADIUS; // Angle in radians

        // Rotate the position vector around the side-axis
        let rotation_delta = Quat::from_axis_angle(axis, -angle);
        let new_pos = rotation_delta * current_pos;
        let new_up = new_pos.normalize();

        // Apply new position
        transform.translation = new_pos;

        // Re-orient the player to stand upright on the new position
        // Calculate a new forward that is tangent to the sphere
        let new_forward = rotation_delta * forward;

        transform.look_at(new_pos + new_forward, new_up);

        // Apply velocity along the normal (away from center)
        transform.translation += new_up * vel.y * dt;

        // Increment fuel while grounded
        let is_grounded = transform.translation.length_squared() <= SPHERE_RADIUS_SQ + 0.02;
        if is_grounded && player.fuel <= 100.0 {
            player.fuel += FUEL_REGEN * dt;
        }

        // Snap player to sphere radius if they're below
        if transform.translation.length_squared() < SPHERE_RADIUS_SQ {
            transform.translation = new_up * SPHERE_RADIUS;
            vel.y = 0.0;
        }
    }
}

fn manage_trail(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    players: Query<(&mut Transform, &mut Player), With<Player>>,
    time: Res<Time>,
) {
    for (transform, mut player) in players {
        // Calculate distance since last segment
        let dist = transform.translation.distance(player.last_trail_pos);

        if dist > TRAIL_SPAWN_DIST {
            // Calculate the midpoint between current and last position
            let midpoint = ((transform.translation + player.last_trail_pos) / 2.0)
                + (TRAIL_RADIUS * transform.up());

            // Direction from last to current
            let direction = (transform.translation - player.last_trail_pos).normalize();

            // Create a rotation that points the Cylinder's Y-axis (top)
            // toward the movement direction
            let rotation = Quat::from_rotation_arc(Vec3::Y, direction);

            let last_spawned = commands
                .spawn((
                    DespawnOnExit(GameState::Playing),
                    Mesh3d(meshes.add(Cylinder::new(TRAIL_RADIUS, TRAIL_RADIUS))),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: PLAYER_COLOR[player.handle],
                        ..default()
                    })),
                    Transform {
                        translation: midpoint,
                        rotation,
                        ..default()
                    },
                    TrailSegment {
                        created_at: time.elapsed_secs_f64(),
                    },
                ))
                .add_rollback()
                .id();

            // Update the last spawn position to current position
            player.last_trail_pos = transform.translation;
            player.last_trail = Some(last_spawned);
        }
    }
}

fn check_collisions(
    mut commands: Commands,
    players: Query<(Entity, &Transform, &Player), With<Player>>,
    trails: Query<(&Transform, &TrailSegment), With<TrailSegment>>,
    mut next_state: ResMut<NextState<RollbackState>>,
    time: Res<Time>,
) {
    let num_players = players.count();

    for (entity, player_trans, _player) in players {
        for (trail_transform, segment) in trails {
            if time.elapsed_secs_f64() - segment.created_at < MIN_TRAIL_LIFE {
                // Don't collide with own most recently spawned segment
                continue;
            }

            let p = player_trans.translation;
            let b = trail_transform.translation;

            // We need the direction the trail is pointing to find the ends
            // Since you used Quat::from_rotation_arc(Vec3::Y, direction),
            // the trail's local Y axis is its "length"
            let trail_dir = trail_transform.up();
            let half_height = TRAIL_SPAWN_DIST / 2.0;

            let start = b - trail_dir * half_height;
            let end = b + trail_dir * half_height;

            // Calculate distance from point P to segment [start, end]
            let distance = dist_to_segment(p, start, end);

            if distance < (TRAIL_RADIUS + PLAYER_RADIUS) {
                // You can refine this logic to be more strict
                // or use actual Hitboxes if using a physics engine.
                // For a first draft, simple distance is great.

                commands.entity(entity).despawn();
                if num_players - 1 <= 1 {
                    // 0 or 1 player left, game over
                    next_state.set(RollbackState::RoundEnd);
                }
            }
        }
    }
}

fn dist_to_segment(p: Vec3, a: Vec3, b: Vec3) -> f32 {
    let v = b - a;
    let w = p - a;
    let c1 = w.dot(v);
    if c1 <= 0.0 {
        return p.distance(a);
    }
    let c2 = v.dot(v);
    if c2 <= c1 {
        return p.distance(b);
    }
    let b2 = c1 / c2;
    let pb = a + v * b2;
    p.distance(pb)
}

fn round_end_timeout(
    mut timer: ResMut<RoundEndTimer>,
    mut state: ResMut<NextState<RollbackState>>,
    time: Res<Time>,
) {
    timer.tick(time.delta());

    if timer.just_finished() {
        state.set(RollbackState::InRound);
    }
}

#[allow(clippy::type_complexity)]
fn move_camera(
    local_players: Res<LocalPlayers>,
    mut transforms: ParamSet<(
        Single<&mut Transform, With<Camera3d>>,
        Query<(&mut Transform, &mut Velocity, &Player), With<Rollback>>,
    )>,
) {
    // Find local player's transform or return
    let Some(player_transform) = transforms
        .p1()
        .iter()
        .find_map(|(transform, _, p)| local_players.0.contains(&p.handle).then_some(transform))
        .copied()
    else {
        return;
    };

    let mut cam_transform = transforms.p0();

    let player_pos = player_transform.translation;
    let player_up = player_pos.normalize_or_zero();

    // Position camera 10 units "back" and 4 units "up" relative to player's current orientation
    let backwards = -player_transform.forward();
    let cam_pos = player_pos + (backwards * 8.0) + (player_up * 6.0);

    cam_transform.translation = cam_pos;
    // Look at the player, keeping the planet's "Up" as the camera's "Up"
    cam_transform.look_at(player_pos, player_up);
}
