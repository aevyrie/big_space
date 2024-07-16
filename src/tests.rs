use bevy::prelude::*;

use crate::{BigSpacePlugin, BigSpaceRootBundle, FloatingOrigin, GridCell};

#[test]
fn changing_floating_origin_updates_global_transform() {
    let mut app = App::new();
    app.add_plugins(BigSpacePlugin::<i32>::default());

    let first = app
        .world_mut()
        .spawn((
            TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                150.0, 0.0, 0.0,
            ))),
            GridCell::<i32>::new(5, 0, 0),
            FloatingOrigin,
        ))
        .id();

    let second = app
        .world_mut()
        .spawn((
            TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                0.0, 0.0, 300.0,
            ))),
            GridCell::<i32>::new(0, -15, 0),
        ))
        .id();

    app.world_mut()
        .spawn(BigSpaceRootBundle::<i32>::default())
        .push_children(&[first, second]);

    app.update();

    app.world_mut().entity_mut(first).remove::<FloatingOrigin>();
    app.world_mut().entity_mut(second).insert(FloatingOrigin);

    app.update();

    let second_global_transform = app.world_mut().get::<GlobalTransform>(second).unwrap();

    assert_eq!(
        second_global_transform.translation(),
        Vec3::new(0.0, 0.0, 300.0)
    );
}

#[test]
fn child_global_transforms_are_updated_when_floating_origin_changes() {
    let mut app = App::new();
    app.add_plugins(BigSpacePlugin::<i32>::default());

    let first = app
        .world_mut()
        .spawn((
            TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                150.0, 0.0, 0.0,
            ))),
            GridCell::<i32>::new(5, 0, 0),
            FloatingOrigin,
        ))
        .id();

    let second = app
        .world_mut()
        .spawn((
            TransformBundle::from_transform(Transform::from_translation(Vec3::new(
                0.0, 0.0, 300.0,
            ))),
            GridCell::<i32>::new(0, -15, 0),
        ))
        .with_children(|parent| {
            parent.spawn((TransformBundle::from_transform(
                Transform::from_translation(Vec3::new(0.0, 0.0, 300.0)),
            ),));
        })
        .id();

    app.world_mut()
        .spawn(BigSpaceRootBundle::<i32>::default())
        .push_children(&[first, second]);

    app.update();

    app.world_mut().entity_mut(first).remove::<FloatingOrigin>();
    app.world_mut().entity_mut(second).insert(FloatingOrigin);

    app.update();

    let child = app.world_mut().get::<Children>(second).unwrap()[0];
    let child_transform = app.world_mut().get::<GlobalTransform>(child).unwrap();

    assert_eq!(child_transform.translation(), Vec3::new(0.0, 0.0, 600.0));
}
