use bevy::prelude::*;

pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player)
            .add_systems(FixedUpdate, move_player);
    }
}


#[derive(Component)]
struct Player;

#[derive(Component)]
struct Speed(f32);



fn spawn_player(
    mut commands : Commands,
    assets : Res<AssetServer>
) {
    commands.spawn((
        Player,
        Speed(10.0),
        Sprite {
            custom_size: Some(Vec2::splat(64.0)),
            ..Sprite::from_image(assets.load("a.png"))
        },
        Transform::from_xyz(0.0, 0.0, 0.0)
    ));
}

fn move_player(
    mut query : Query<(&mut Transform, &Speed), With<Player>>,
    keys : Res<ButtonInput<KeyCode>>,
) {
    let mut dir = Vec2::ZERO;

    if keys.pressed(KeyCode::KeyW) { dir.y += 1.0 }
    if keys.pressed(KeyCode::KeyS) { dir.y -= 1.0 }
    if keys.pressed(KeyCode::KeyA) { dir.x -= 1.0 }
    if keys.pressed(KeyCode::KeyD) { dir.x += 1.0 }

    let dir = dir.normalize_or_zero();

    for (mut transform, speed) in &mut query {
        transform.translation += (dir * speed.0).extend(0.0)
    }
}