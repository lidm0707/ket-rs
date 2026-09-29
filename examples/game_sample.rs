//! Bevy game driven by ket: the enemy AI asks the engine a typed question
//! every tick — (latent state, "which behavior?") -> choice — and moves
//! accordingly. No training, no hand-coded if/else AI tree.
//!
//! Controls: arrows / WASD move the player. Collect coins, avoid the enemy.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ket_rs::decision::{KetDecision, KetEngine, KetQuery};
use ket_rs::latent::{FLAG_SAFE, LatentState, STATE_DIM};
use ket_rs::question::{ChoiceId, TypedQuestion, Urgency};

/// Normalization bounds for latent dims.
const ARENA_HALF: f32 = 640.0;
const THREAT_RADIUS: f32 = 240.0;
const THREAT_RADIUS_SQ: f32 = THREAT_RADIUS * THREAT_RADIUS;
/// Evidence per dim: threat proximity and player health pressure.
const THREAT_BUMP: f32 = 2.0;
const HURT_BUMP: f32 = 1.5;
const SPEED: f32 = 220.0;
const ENEMY_SPEED: f32 = 140.0;
const ENTITY_RADIUS: f32 = 24.0;
const COIN_COUNT: usize = 6;
const CHOICE_COUNT: usize = 5;
const KEYWORDS: [&str; 1] = ["general"];
/// Wander heading sweep, radians per second.
const ROTATION_RATE: f32 = 90.0_f32.to_radians();
/// Flee never retreats past the coin ring.
const FLEE_RING_RADIUS: f32 = 300.0;
/// Usable fraction of the window so sprites stay fully visible.
const SCREEN_MARGIN: f32 = 0.95;
/// Player stops just short of the edge too (shares the safe margin).
const PLAYER_MARGIN: f32 = SCREEN_MARGIN;
/// Evidence bump when a coin is within loot radius.
const COIN_BUMP: f32 = 1.0;
const LOOT_RADIUS: f32 = 260.0;
const LOOT_RADIUS_SQ: f32 = LOOT_RADIUS * LOOT_RADIUS;
/// Preferred distance the orbit keeps from a coin.
const ORBIT_RADIUS: f32 = 80.0;
/// Inward correction speed while orbiting, fraction of movement.
const ORBIT_PULL: f32 = 0.3;
const COIN_SPEED: f32 = 90.0;
const COIN_DIAM: f32 = 2.0 * ENTITY_RADIUS;
const COIN_DIAM_SQ: f32 = COIN_DIAM * COIN_DIAM;
/// Fraction of COIN_DIAM the enemy is pushed out per second while inside
/// a coin — soft, so the AI direction keeps applying.
const COIN_PUSH_RATE: f32 = 4.0;
/// HP lost per coin touch.
const COIN_HIT_DAMAGE: f32 = 1.0;
/// Seconds of immunity between coin hits.
const COIN_HIT_COOLDOWN: f32 = 1.0;
/// Extra loot evidence when an incoming coin closes inside this radius —
/// the trigger zone the Evade action reads.
const EVADE_RADIUS: f32 = 150.0;
const EVADE_RADIUS_SQ: f32 = EVADE_RADIUS * EVADE_RADIUS;
const EVADE_BUMP: f32 = 2.0;
/// How far ahead (s) Evade predicts an incoming coin's position.
const EVADE_LOOKAHEAD_SECS: f32 = 1.3;
/// Center pull blended into Evade so it never gets cornered on a wall.
const EVADE_CENTER_PULL: f32 = 0.35;
/// Evade triggers only on a real approach, not a drift: fraction of
/// COIN_SPEED heading at the enemy.
const EVADE_CLOSING_MIN: f32 = 0.25;

/// Time-as-HP: the enemy's blood pool and its drain rate per second.
const ENEMY_MAX_HP: f32 = 10.0;
const HP_DECAY_PER_SEC: f32 = 0.25;
/// Blink sharpness (rad/s) and duration after an HP tick.
const FLASH_RATE: f32 = 20.0;
const FLASH_SECS: f32 = 0.6;
/// Extra speed multiplier at zero HP (desperation sprint).
const LOW_HP_SPEED_BOOST: f32 = 1.0;
/// Below this HP ratio the enemy panics — latent desperation evidence.
const PANIC_HP_RATIO: f32 = 0.3;
const HP_BAR_WIDTH: f32 = 2.0 * ENTITY_RADIUS;
const HP_BAR_HEIGHT: f32 = 6.0;
const HP_BAR_OFFSET: f32 = 38.0;
/// Player HP bar geometry (same style as the enemy's).
const PLAYER_HP_MAX: u8 = 3;
const PLAYER_BAR_OFFSET: f32 = 38.0;
const ENEMY_COLOR: Color = Color::srgb(1.0, 0.3, 0.3);
const FLASH_COLOR: Color = Color::srgb(1.0, 0.95, 0.9);
/// Movement graph: trail marker cadence, lifetime, and size.
const TRAIL_INTERVAL_SECS: f32 = 0.12;
const TRAIL_LIFETIME: f32 = 1.5;
const TRAIL_SIZE: f32 = 5.0;
const TRAIL_COLOR: Color = Color::srgba(0.5, 0.7, 1.0, 0.5);

/// Enemy behaviors the ket engine picks between.
#[derive(Debug, Clone, Copy, PartialEq)]
enum AiAction {
    Chase,
    Wander,
    Flee,
    Orbit,
    Evade,
}

/// Per-choice direction rows over `[threat hurt cornered loot dx dy]` —
/// each action reads the evidence it cares about; the argmax of
/// logistic(dot) picks the winner.
const ACTION_DIRECTIONS: [[f32; STATE_DIM]; CHOICE_COUNT] = [
    [0.9, 0.0, -0.3, -0.05, 0.0, 0.0], // Chase: primary goal — hunt the player
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],    // Wander: neutral
    [0.9, 0.6, 0.6, -0.5, 0.0, 0.0],   // Flee: threat + hurt + cornered
    [-0.4, -0.4, -0.4, -0.3, 1.2, 1.2], // Orbit: calm coins, strong bearing pull
    [0.0, 0.0, 0.0, 1.0, 0.0, 0.0],    // Evade: strong incoming-loot evidence
];

impl AiAction {
    const ALL: [AiAction; CHOICE_COUNT] = [
        AiAction::Chase,
        AiAction::Wander,
        AiAction::Flee,
        AiAction::Orbit,
        AiAction::Evade,
    ];

    fn choice(self) -> ChoiceId {
        ChoiceId(match self {
            AiAction::Chase => 0,
            AiAction::Wander => 1,
            AiAction::Flee => 2,
            AiAction::Orbit => 3,
            AiAction::Evade => 4,
        })
    }

    fn from_choice(id: ChoiceId) -> Self {
        Self::ALL[id.0 as usize % CHOICE_COUNT]
    }
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Enemy {
    dims: [f32; STATE_DIM],
    heading: Vec2,
    hp: f32,
    flash: f32,
    /// Seconds of immunity left after a coin hit.
    coin_invuln: f32,
}

#[derive(Component)]
struct Coin;

#[derive(Component)]
struct Velocity(Vec2);

/// HP gauge above the enemy; shrinks with the ratio.
#[derive(Component)]
struct HpBar;

/// Fading breadcrumb dot marking the enemy's path.
#[derive(Component)]
struct Trail {
    age: f32,
}

/// Coin view: reads only — disjoint from the enemy's mutable access.
type CoinQuery<'w, 's> =
    Query<'w, 's, (&'static Transform, &'static Velocity), (With<Coin>, Without<Enemy>)>;

#[derive(Component)]
struct Health {
    hp: u8,
    /// Seconds of immunity left after a hit.
    invuln: f32,
}

/// Immunity window after taking a hit, so HP can't drain per-frame.
const IFRAME_SECS: f32 = 1.5;
/// Center distance that counts as collecting a coin.
const PICKUP_RADIUS: f32 = 1.8 * ENTITY_RADIUS;
/// How fast player velocity approaches the input target (1/s).
const PLAYER_ACCEL: f32 = 6.0;

#[derive(Resource)]
struct Score(u32);

/// Game phase: Playing runs the sim; Won/Lost freeze it behind an overlay.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum Phase {
    #[default]
    Playing,
    Won,
    Lost,
}

/// Full-screen result text + New Game hint.
#[derive(Component)]
struct GameOverlay;

/// Small always-on control hint in the corner.
#[derive(Component)]
struct ControlHint;

/// HP gauge above the player.
#[derive(Component)]
struct PlayerHpBar;

/// Everything a restart wipes: players, enemy, coins, trails.
type GameBodyQuery<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        Without<Camera>,
        Without<GameOverlay>,
        Or<(With<Player>, With<Enemy>, With<Coin>, With<Trail>)>,
    ),
>;

/// Overlay font size.
const OVERLAY_FONT_SIZE: f32 = 48.0;

/// KetEngine isn't a Bevy Resource; wrap it and forward the hot path.
#[derive(Resource)]
struct Ai(KetEngine);

impl std::ops::Deref for Ai {
    type Target = KetEngine;
    fn deref(&self) -> &KetEngine {
        &self.0
    }
}

impl std::ops::DerefMut for Ai {
    fn deref_mut(&mut self) -> &mut KetEngine {
        &mut self.0
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(Ai(KetEngine::default()))
        .insert_resource(Score(0))
        .init_state::<Phase>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                player_move,
                coin_move,
                coin_collide,
                encode_state,
                enemy_decide,
                enemy_coin_damage,
                enemy_hp,
                hp_bar,
                player_hp_bar,
                spawn_trail,
                fade_trail,
                coin_pickup,
                hurt_player,
                enemy_coin_collide,
                check_end,
            )
                .chain()
                .run_if(in_state(Phase::Playing)),
        )
        .add_systems(Update, (update_overlay, new_game))
        .run();
}

/// Vertical offset of the name label above a sprite.
const LABEL_OFFSET: f32 = 40.0;
/// Font size of the name label.
const LABEL_FONT_SIZE: f32 = 24.0;

fn spawn_label(parent: &mut ChildSpawnerCommands, text: &str) {
    parent.spawn((
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(LABEL_FONT_SIZE),
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(0.0, ENTITY_RADIUS + LABEL_OFFSET, 0.0),
    ));
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    spawn_game(&mut commands);
    commands.spawn((
        GameOverlay,
        Text2d::new(""),
        TextFont {
            font_size: FontSize::Px(OVERLAY_FONT_SIZE),
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(0.0, 0.0, 10.0),
        Visibility::Hidden,
    ));
    commands.spawn((
        ControlHint,
        Text2d::new("WASD move · R new game (after end) · Esc restart"),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5)),
        Transform::from_xyz(0.0, -ARENA_HALF - 40.0, 10.0),
    ));
}

/// All in-match entities: player, enemy, coins (with children).
fn spawn_game(commands: &mut Commands) {
    commands
        .spawn((
            Player,
            Health {
                hp: PLAYER_HP_MAX,
                invuln: 0.0,
            },
            Velocity(Vec2::ZERO),
            Sprite::from_color(Color::srgb(0.2, 0.6, 1.0), Vec2::splat(2.0 * ENTITY_RADIUS)),
            Transform::from_xyz(0.0, -200.0, 0.0),
        ))
        .with_children(|c| {
            spawn_label(c, "Player");
            c.spawn((
                PlayerHpBar,
                Sprite::from_color(
                    Color::srgb(0.2, 1.0, 0.3),
                    Vec2::new(HP_BAR_WIDTH, HP_BAR_HEIGHT),
                ),
                Transform::from_xyz(0.0, PLAYER_BAR_OFFSET, 0.0),
            ));
        });
    commands
        .spawn((
            Enemy {
                dims: [0.0; STATE_DIM],
                heading: Vec2::X,
                hp: ENEMY_MAX_HP,
                flash: 0.0,
                coin_invuln: 0.0,
            },
            Sprite::from_color(ENEMY_COLOR, Vec2::splat(ENTITY_RADIUS)),
            Transform::from_xyz(0.0, 200.0, 0.0),
        ))
        .with_children(|c| {
            spawn_label(c, "Enemy");
            c.spawn((
                HpBar,
                Sprite::from_color(
                    Color::srgb(0.2, 1.0, 0.3),
                    Vec2::new(HP_BAR_WIDTH, HP_BAR_HEIGHT),
                ),
                Transform::from_xyz(0.0, HP_BAR_OFFSET, 0.0),
            ));
        });
    for i in 0..COIN_COUNT {
        let angle = i as f32 / COIN_COUNT as f32 * std::f32::consts::TAU;
        commands.spawn((
            Coin,
            Velocity(Vec2::from_angle(angle + std::f32::consts::FRAC_PI_2)),
            Sprite::from_color(Color::srgb(1.0, 0.85, 0.1), Vec2::splat(ENTITY_RADIUS)),
            Transform::from_xyz(angle.cos() * 300.0, angle.sin() * 300.0, 0.0),
        ));
    }
}

fn clamp_to_window(pos: &mut Vec3, window: &Window, margin: f32) -> bool {
    let half = 0.5 * window.size() * margin;
    let hit_x = pos.x < -half.x || pos.x > half.x;
    let hit_y = pos.y < -half.y || pos.y > half.y;
    pos.x = pos.x.clamp(-half.x, half.x);
    pos.y = pos.y.clamp(-half.y, half.y);
    hit_x || hit_y
}

fn coin_move(
    window: Single<&Window, With<PrimaryWindow>>,
    mut coins: Query<(&mut Transform, &mut Velocity), With<Coin>>,
    time: Res<Time>,
) {
    let window = window.into_inner();
    for (mut ct, mut vel) in &mut coins {
        ct.translation += vel.0.extend(0.0) * COIN_SPEED * time.delta_secs();
        if clamp_to_window(&mut ct.translation, window, SCREEN_MARGIN) {
            vel.0 = -vel.0;
        }
    }
}

/// Equal-mass elastic collision: swap velocities and separate overlap.
fn coin_collide(mut coins: Query<(&mut Transform, &mut Velocity), With<Coin>>) {
    let mut items: Vec<_> = coins.iter_mut().collect();
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            let (left, right) = items.split_at_mut(j);
            let (ta, va) = (&mut *left[i].0, &mut *left[i].1);
            let (tb, vb) = (&mut *right[0].0, &mut *right[0].1);
            let d = tb.translation - ta.translation;
            let dist_sq = d.length_squared();
            if dist_sq >= COIN_DIAM_SQ || dist_sq == 0.0 {
                continue;
            }
            let normal = d.normalize();
            let overlap = COIN_DIAM - dist_sq.sqrt();
            ta.translation -= normal * (0.5 * overlap);
            tb.translation += normal * (0.5 * overlap);
            std::mem::swap(&mut va.0, &mut vb.0);
        }
    }
}

fn player_move(
    input: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
    player: Single<(&mut Transform, &mut Velocity), With<Player>>,
) {
    let (mut pt, mut vel) = player.into_inner();
    let mut dir = Vec2::ZERO;
    for (key, d) in [
        (KeyCode::ArrowUp, Vec2::Y),
        (KeyCode::KeyW, Vec2::Y),
        (KeyCode::ArrowDown, Vec2::NEG_Y),
        (KeyCode::KeyS, Vec2::NEG_Y),
        (KeyCode::ArrowLeft, Vec2::NEG_X),
        (KeyCode::KeyA, Vec2::NEG_X),
        (KeyCode::ArrowRight, Vec2::X),
        (KeyCode::KeyD, Vec2::X),
    ] {
        if input.pressed(key) {
            dir += d;
        }
    }
    if dir != Vec2::ZERO {
        dir = dir.normalize();
    }
    // Ease toward the input target — smooth accel and coast-down.
    let target = dir * SPEED;
    let blend = 1.0 - (-PLAYER_ACCEL * time.delta_secs()).exp();
    let delta = (target - vel.0) * blend;
    vel.0 += delta;
    pt.translation += vel.0.extend(0.0) * time.delta_secs();
    clamp_to_window(&mut pt.translation, window.into_inner(), PLAYER_MARGIN);
}

/// Rule pass: write evidence into the enemy's latent dims.
///
/// Layout: `[threat] [hurt] [cornered] [loot] [coin_dx] [coin_dy]` — dims
/// 0..2 push toward Flee, loot supports Orbit (amplified when the coin's
/// per-frame motion closes on the enemy), coin_dx/dy carry the signed unit
/// direction to the nearest coin so the sector score is bearing-aware.
fn encode_state(
    player: Single<&Transform, With<Player>>,
    enemy: Single<(&Transform, &mut Enemy), Without<Player>>,
    health: Single<&Health, With<Player>>,
    coins: CoinQuery,
) {
    let (pt, (et, mut brain)) = (player.into_inner(), enemy.into_inner());
    let to_player = pt.translation.xy() - et.translation.xy();
    let near = to_player.length_squared() < THREAT_RADIUS_SQ;

    brain.dims = [0.0; STATE_DIM];
    if near {
        brain.dims[0] = THREAT_BUMP;
    }
    if health.hp <= 1 {
        brain.dims[1] = HURT_BUMP;
    }
    // Desperation: a drained enemy pushes the same panic dim as a hurt player.
    if brain.hp / ENEMY_MAX_HP < PANIC_HP_RATIO {
        brain.dims[1] = HURT_BUMP;
    }
    let cornered = et.translation.abs().max_element() > ARENA_HALF - THREAT_RADIUS;
    brain.dims[2] = u8::from(near && cornered) as f32 * THREAT_BUMP;

    let bearing = nearest_coin(&et.translation.xy(), &coins);
    if let Some((ct, cv)) = bearing {
        let to_coin = ct.translation.xy() - et.translation.xy();
        let dist_sq = to_coin.length_squared();
        let dir = to_coin.normalize_or_zero();
        // Loot evidence only counts inside the loot ring — otherwise the
        // Evade row dominates from across the arena.
        if dist_sq < LOOT_RADIUS_SQ {
            // Graded closing: how hard is this coin actually incoming?
            let closing = (-cv.0.dot(dir) / COIN_SPEED).clamp(0.0, 1.0);
            let mut loot = COIN_BUMP + COIN_BUMP * closing;
            // Panic evidence only for a real threat: close AND fast.
            if dist_sq < EVADE_RADIUS_SQ && closing >= EVADE_CLOSING_MIN {
                loot += EVADE_BUMP;
            }
            brain.dims[3] = loot;
            brain.dims[4] = dir.x;
            brain.dims[5] = dir.y;
        }
    }
}

/// The ket decision: plan + decide per tick via the zero-alloc hot path.
fn nearest_coin<'a>(pos: &Vec2, coins: &'a CoinQuery) -> Option<(&'a Transform, &'a Velocity)> {
    coins.iter().min_by(|a, b| {
        let da = pos.distance_squared(a.0.translation.xy());
        let db = pos.distance_squared(b.0.translation.xy());
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn enemy_decide(
    mut engine: ResMut<Ai>,
    player: Single<&Transform, With<Player>>,
    window: Single<&Window, With<PrimaryWindow>>,
    enemy: Single<(&mut Transform, &mut Enemy), Without<Player>>,
    coins: CoinQuery,
    time: Res<Time>,
    mut log_timer: Local<f32>,
) {
    let (mut et, mut brain) = enemy.into_inner();
    let to_player = player.translation.xy() - et.translation.xy();
    let choices: Vec<ChoiceId> = AiAction::ALL.iter().map(|a| a.choice()).collect();
    let state = LatentState {
        dims: &brain.dims,
        flags: FLAG_SAFE,
    };
    let query = KetQuery {
        state: &state,
        question: TypedQuestion::PickWeighted {
            keywords: &KEYWORDS,
            choices: &choices,
            directions: &ACTION_DIRECTIONS,
        },
    };
    let mut choices_out = [ChoiceId(u16::MAX); CHOICE_COUNT];
    let mut scores_out = [0.0_f32; CHOICE_COUNT];

    let decision: KetDecision = match engine.decide_into(query, &mut choices_out, &mut scores_out) {
        Ok(d) => d,
        Err(e) => {
            warn!("ket rejected state: {e:?}");
            return;
        }
    };

    let action = AiAction::from_choice(decision.choice);
    let dir = match action {
        AiAction::Chase => to_player.normalize_or_zero(),
        AiAction::Flee => {
            brain.heading = -to_player.normalize_or_zero();
            brain.heading
        }
        AiAction::Wander => {
            brain.heading =
                Vec2::from_angle(brain.heading.to_angle() + time.delta_secs() * ROTATION_RATE)
                    .normalize_or_zero();
            brain.heading
        }
        AiAction::Orbit => match nearest_coin(&et.translation.xy(), &coins) {
            Some(ct) => {
                let radial = et.translation.xy() - ct.0.translation.xy();
                let tangent = radial.perp().normalize_or_zero();
                let pull = radial.normalize_or_zero()
                    * ((radial.length() - ORBIT_RADIUS) / ORBIT_RADIUS).clamp(-1.0, 1.0)
                    * ORBIT_PULL;
                tangent + pull
            }
            None => Vec2::ZERO,
        },
        AiAction::Evade => match nearest_coin(&et.translation.xy(), &coins) {
            Some((ct, cv)) => {
                // Run from the coin's predicted position — stable direction,
                // no per-frame sign flips. Center pull keeps the dodge off
                // the walls so it can't be cornered.
                let predicted = ct.translation.xy() + cv.0 * EVADE_LOOKAHEAD_SECS;
                let away = (et.translation.xy() - predicted).normalize_or_zero();
                (away - et.translation.xy().normalize_or_zero() * EVADE_CENTER_PULL)
                    .normalize_or_zero()
            }
            None => Vec2::ZERO,
        },
    };
    // Urgency scales aggression; low HP adds a desperation sprint.
    let urgency_boost: f32 = match decision.urgency {
        Urgency::Routine => 1.0,
        Urgency::Elevated => 1.25,
        Urgency::Critical => 1.6,
    };
    let hp_ratio = (brain.hp / ENEMY_MAX_HP).clamp(0.0, 1.0);
    let speed = ENEMY_SPEED * (1.0 + LOW_HP_SPEED_BOOST * (1.0 - hp_ratio));
    et.translation += (dir * speed * urgency_boost * time.delta_secs()).extend(0.0);
    *log_timer += time.delta_secs();
    if *log_timer >= 1.0 {
        *log_timer = 0.0;
        eprintln!(
            "?{:?} dir=({:.2},{:.2}) speed={:.0} pos=({:.0},{:.0}) hp={:.1}",
            action, dir.x, dir.y, speed, et.translation.x, et.translation.y, brain.hp
        );
    }
    if action == AiAction::Flee {
        et.translation = et
            .translation
            .xy()
            .clamp_length_max(FLEE_RING_RADIUS)
            .extend(0.0);
    }
    clamp_to_window(&mut et.translation, window.into_inner(), SCREEN_MARGIN);
}

/// Soft separation: nudge the enemy out of a coin over several frames
/// instead of teleporting — the AI's computed direction still applies.
fn enemy_coin_collide(
    time: Res<Time>,
    enemy: Single<&mut Transform, (With<Enemy>, Without<Player>)>,
    coins: Query<&Transform, (With<Coin>, Without<Enemy>)>,
) {
    let mut et = enemy.into_inner();
    let push = COIN_DIAM * COIN_PUSH_RATE * time.delta_secs();
    for ct in &coins {
        let d = et.translation - ct.translation;
        let dist_sq = d.length_squared();
        if dist_sq >= COIN_DIAM_SQ || dist_sq == 0.0 {
            continue;
        }
        let dist = dist_sq.sqrt();
        if dist + push < COIN_DIAM {
            et.translation += d.normalize() * push;
        }
    }
}

/// Time-as-HP: the enemy bleeds with the clock, blinks on each HP tick,
/// and respawns at full blood when drained.
fn enemy_hp(time: Res<Time>, enemy: Single<(&mut Enemy, &mut Sprite), With<Enemy>>) {
    let (e, sprite) = enemy.into_inner();
    let e = e.into_inner();
    let sprite = sprite.into_inner();
    let before = e.hp;
    e.hp = (e.hp - HP_DECAY_PER_SEC * time.delta_secs()).max(0.0);
    e.flash = (e.flash - time.delta_secs()).max(0.0);
    if before.floor() > e.hp.floor() {
        e.flash = FLASH_SECS;
    }
    if e.hp <= 0.0 {
        info!("enemy drained, respawning");
        e.hp = ENEMY_MAX_HP;
    }
    sprite.color = if e.flash > 0.0 && (e.flash * FLASH_RATE).sin() > 0.0 {
        FLASH_COLOR
    } else {
        ENEMY_COLOR
    };
}

/// Mutable sprite+transform view of a gauge bar.
type BarQuery<'w, 's, F> = Query<'w, 's, (&'static mut Sprite, &'static mut Transform), F>;

/// The gauge shrinks left-anchored and shifts color green -> red.
fn hp_bar(enemy: Single<&Enemy, With<Enemy>>, mut bars: BarQuery<With<HpBar>>) {
    let ratio = enemy.into_inner().hp / ENEMY_MAX_HP;
    for (mut sprite, mut tf) in &mut bars {
        sprite.custom_size = Some(Vec2::new(HP_BAR_WIDTH * ratio, HP_BAR_HEIGHT));
        tf.translation.x = -0.5 * HP_BAR_WIDTH * (1.0 - ratio);
        sprite.color = Color::srgb(1.0 - ratio, ratio, 0.1);
    }
}

/// Player's gauge: hp/3, same shrink-and-shift style.
fn player_hp_bar(
    player: Single<&Health, With<Player>>,
    mut bars: BarQuery<(With<PlayerHpBar>, Without<HpBar>)>,
) {
    let ratio = f32::from(player.into_inner().hp) / f32::from(PLAYER_HP_MAX);
    for (mut sprite, mut tf) in &mut bars {
        sprite.custom_size = Some(Vec2::new(HP_BAR_WIDTH * ratio, HP_BAR_HEIGHT));
        tf.translation.x = -0.5 * HP_BAR_WIDTH * (1.0 - ratio);
        sprite.color = Color::srgb(1.0 - ratio, ratio, 0.1);
    }
}

/// Movement graph: drop a breadcrumb at the enemy position on a cadence.
fn spawn_trail(
    mut commands: Commands,
    time: Res<Time>,
    enemy: Single<&Transform, With<Enemy>>,
    mut cooldown: Local<f32>,
) {
    *cooldown += time.delta_secs();
    if *cooldown < TRAIL_INTERVAL_SECS {
        return;
    }
    *cooldown = 0.0;
    commands.spawn((
        Trail { age: 0.0 },
        Sprite::from_color(TRAIL_COLOR, Vec2::splat(TRAIL_SIZE)),
        Transform::from_translation(enemy.translation),
    ));
}

/// Fade each breadcrumb out, then despawn it.
fn fade_trail(
    mut commands: Commands,
    time: Res<Time>,
    mut trails: Query<(Entity, &mut Trail, &mut Sprite)>,
) {
    for (entity, mut trail, mut sprite) in &mut trails {
        trail.age += time.delta_secs();
        if trail.age >= TRAIL_LIFETIME {
            commands.entity(entity).despawn();
            continue;
        }
        let fade = 1.0 - trail.age / TRAIL_LIFETIME;
        sprite.color = Color::srgba(0.5, 0.7, 1.0, 0.5 * fade);
    }
}

/// Mutable view of the enemy for hit processing.
type EnemyHitQuery<'w, 's> =
    Query<'w, 's, (&'static mut Enemy, &'static Transform), (With<Enemy>, Without<Player>)>;

/// Touching a coin costs the enemy blood — coins are hazards to it, not
/// pickups. The coin stays; only the enemy bleeds.
fn enemy_coin_damage(
    time: Res<Time>,
    mut enemy: EnemyHitQuery,
    coins: Query<&Transform, (With<Coin>, Without<Enemy>)>,
) {
    let (e, et) = enemy.single_mut().expect("exactly one enemy");
    let (e, et) = (e.into_inner(), *et);
    e.coin_invuln = (e.coin_invuln - time.delta_secs()).max(0.0);
    if e.coin_invuln > 0.0 {
        return;
    }
    for ct in &coins {
        if et.translation.distance_squared(ct.translation) >= COIN_DIAM_SQ {
            continue;
        }
        e.hp = (e.hp - COIN_HIT_DAMAGE).max(0.0);
        e.coin_invuln = COIN_HIT_COOLDOWN;
        e.flash = FLASH_SECS;
        info!("coin hit! enemy hp={:.1}", e.hp);
        break;
    }
}

/// Win when every coin is banked; lose when the player's blood runs out.
fn check_end(
    coins: Query<Entity, With<Coin>>,
    health: Single<&Health, With<Player>>,
    mut next: ResMut<NextState<Phase>>,
) {
    if coins.is_empty() {
        next.set(Phase::Won);
    } else if health.into_inner().hp == 0 {
        next.set(Phase::Lost);
    }
}

/// Show the result text behind the frozen match.
fn update_overlay(
    phase: Res<State<Phase>>,
    overlay: Single<(&mut Text2d, &mut Visibility), With<GameOverlay>>,
) {
    let (mut text, mut vis) = overlay.into_inner();
    match phase.get() {
        Phase::Playing => *vis = Visibility::Hidden,
        Phase::Won => {
            text.0 = "YOU WIN!\nPress R for new game".to_string();
            *vis = Visibility::Visible;
        }
        Phase::Lost => {
            text.0 = "YOU LOSE\nPress R for new game".to_string();
            *vis = Visibility::Visible;
        }
    }
}

/// R restarts a finished match; Esc restarts anytime — wipe entities,
/// respawn, back to Playing.
fn new_game(
    mut commands: Commands,
    input: Res<ButtonInput<KeyCode>>,
    phase: Res<State<Phase>>,
    mut next: ResMut<NextState<Phase>>,
    mut score: ResMut<Score>,
    bodies: GameBodyQuery,
) {
    let restart_anytime = input.just_pressed(KeyCode::Escape);
    let restart_after_end = input.just_pressed(KeyCode::KeyR) && phase.get() != &Phase::Playing;
    if !(restart_anytime || restart_after_end) {
        return;
    }
    for entity in &bodies {
        commands.entity(entity).try_despawn();
    }
    score.0 = 0;
    spawn_game(&mut commands);
    next.set(Phase::Playing);
}

fn coin_pickup(
    mut commands: Commands,
    mut score: ResMut<Score>,
    player: Single<&Transform, With<Player>>,
    coins: Query<(Entity, &Transform), With<Coin>>,
) {
    for (entity, ct) in &coins {
        let d = player.translation.distance_squared(ct.translation);
        if d < PICKUP_RADIUS * PICKUP_RADIUS {
            eprintln!("coin picked, dist={:.0}", d.sqrt());
            commands.entity(entity).despawn();
            score.0 += 1;
            info!("coin! score={}", score.0);
        }
    }
}

fn hurt_player(
    time: Res<Time>,
    health: Single<(&mut Health, &Transform, &mut Sprite), With<Player>>,
    enemy: Single<&Transform, (With<Enemy>, Without<Player>)>,
) {
    let (mut hp, pt, mut sprite) = health.into_inner();
    hp.invuln = (hp.invuln - time.delta_secs()).max(0.0);
    // Blink while immune so the window is readable.
    let a = if hp.invuln > 0.0 && (hp.invuln * FLASH_RATE).sin() > 0.0 {
        0.3
    } else {
        1.0
    };
    sprite.color = Color::srgba(0.2, 0.6, 1.0, a);
    if hp.invuln == 0.0
        && hp.hp > 0
        && pt.translation.distance_squared(enemy.translation) < ENTITY_RADIUS * ENTITY_RADIUS
    {
        hp.hp -= 1;
        hp.invuln = IFRAME_SECS;
        info!("hit! health={}", hp.hp);
        if hp.hp == 0 {
            warn!("game over");
        }
    }
}
