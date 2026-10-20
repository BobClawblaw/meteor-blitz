//! Game simulation: wrapping world, player, weapons, meteors, enemies, bosses.

pub const WORLD_W: f32 = 5760.0;
pub const WORLD_H: f32 = 3240.0;
pub const PLAYER_RADIUS: f32 = 14.0;
pub const PLAYER_MAX_HEALTH: f32 = 150.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Player,
    Meteor,
    Debris,
    Projectile,
    Enemy,
    Boss,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Arcade,
    Survival,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EnemyKind {
    Drone,
    Turret,
    Slinger,
}

#[derive(Clone, Copy)]
pub struct Entity {
    pub id: u64,
    pub faction: Faction,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub radius: f32,
    pub mass: f32,
    pub health: f32,
    pub kind: u8,
    pub weapon: u8,
    pub life: f32,
    pub damage: f32,
    pub pierce: i32,
    pub grace: f32,
    pub rot: f32,
    pub rot_speed: f32,
    pub enemy: EnemyKind,
    pub fire_t: f32,
}

pub struct Player {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub aim_x: f32,
    pub aim_y: f32,
    pub health: f32,
    pub heat: f32,
    pub overheated: bool,
    pub overheat_t: f32,
    pub fire_t: f32,
    pub weapon: u8,
    pub upgrades: [u8; 4],
    pub ore: u32,
    pub bombs: u32,
    pub dash_cd: f32,
    pub dash_t: f32,
    pub dash_dir_x: f32,
    pub dash_dir_y: f32,
    pub invuln: f32,
    pub grabbed: u64,
    pub dead: bool,
}

#[derive(Clone, Copy)]
pub enum Event {
    Fire(u8, f32, f32),
    Shatter(f32, f32, u8, f32),
    PlayerHit(f32),
    Dash(f32, f32),
    Bomb(f32, f32),
    Grab(f32, f32),
    Sling(f32, f32),
    Kill(f32, f32),
}

pub struct Game {
    pub mode: Mode,
    pub world: usize,
    pub stage: usize,
    pub entities: Vec<Entity>,
    pub player: Player,
    pub score: u64,
    pub combo: f32,
    pub combo_idle: f32,
    pub spawn_t: f32,
    pub time: f32,
    pub stage_time: f32,
    pub boss_spawned: bool,
    pub boss_alive: bool,
    pub over: bool,
    pub won: bool,
    pub next_id: u64,
    pub rng: u64,
    pub events: Vec<Event>,
    pub shake: f32,
}

pub fn wrap(v: f32, size: f32) -> f32 {
    ((v % size) + size) % size
}

pub fn wrap_delta(from: f32, to: f32, size: f32) -> f32 {
    let mut d = to - from;
    if d > size * 0.5 {
        d -= size;
    } else if d < -size * 0.5 {
        d += size;
    }
    d
}

impl Game {
    pub fn new(mode: Mode, world: usize, stage: usize) -> Game {
        let mut g = Game {
            mode,
            world,
            stage,
            entities: Vec::new(),
            player: Player {
                x: WORLD_W * 0.5,
                y: WORLD_H * 0.5,
                vx: 0.0,
                vy: 0.0,
                aim_x: 1.0,
                aim_y: 0.0,
                health: PLAYER_MAX_HEALTH,
                heat: 0.0,
                overheated: false,
                overheat_t: 0.0,
                fire_t: 0.0,
                weapon: 0,
                upgrades: [0, 0, 0, 0],
                ore: 0,
                bombs: 3,
                dash_cd: 0.0,
                dash_t: 0.0,
                dash_dir_x: 1.0,
                dash_dir_y: 0.0,
                invuln: 0.0,
                grabbed: 0,
                dead: false,
            },
            score: 0,
            combo: 1.0,
            combo_idle: 0.0,
            spawn_t: 0.0,
            time: 0.0,
            stage_time: 0.0,
            boss_spawned: false,
            boss_alive: false,
            over: false,
            won: false,
            next_id: 1,
            rng: 0x9e3779b97f4a7c15,
            events: Vec::new(),
            shake: 0.0,
        };
        for _ in 0..6 {
            g.spawn_meteor();
        }
        g
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f32 / (1u64 << 53) as f32
    }

    fn rand_range(&mut self, a: f32, b: f32) -> f32 {
        a + self.rand() * (b - a)
    }

    fn spawn(&mut self, faction: Faction, x: f32, y: f32, vx: f32, vy: f32, radius: f32, mass: f32, health: f32) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let rot = self.rand_range(0.0, 6.28);
        let rot_speed = self.rand_range(-1.5, 1.5);
        self.entities.push(Entity {
            id,
            faction,
            x,
            y,
            vx,
            vy,
            radius,
            mass,
            health,
            kind: 0,
            weapon: 0,
            life: 0.0,
            damage: 0.0,
            pierce: 0,
            grace: 0.0,
            rot,
            rot_speed,
            enemy: EnemyKind::Drone,
            fire_t: 0.0,
        });
        self.entities.len() - 1
    }

    pub fn spawn_meteor(&mut self) {
        let mass = self.rand_range(1.0, 6.0);
        let radius = 6.0 + mass * 2.0;
        let speed = self.rand_range(100.0, 220.0);
        let angle = self.rand_range(0.0, 6.283);
        let dist = self.rand_range(700.0, 1100.0);
        let x = wrap(self.player.x + angle.cos() * dist, WORLD_W);
        let y = wrap(self.player.y + angle.sin() * dist, WORLD_H);
        let dx = wrap_delta(x, self.player.x, WORLD_W);
        let dy = wrap_delta(y, self.player.y, WORLD_H);
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let roll = self.rand();
        let kind = if roll < 0.6 { 0 } else if roll < 0.8 { 1 } else { 2 };
        let idx = self.spawn(Faction::Meteor, x, y, dx / len * speed, dy / len * speed, radius, mass, 10.0 + mass * 5.0);
        self.entities[idx].kind = kind;
    }

    fn spawn_enemy(&mut self, kind: EnemyKind) {
        let angle = self.rand_range(0.0, 6.283);
        let dist = self.rand_range(600.0, 1000.0);
        let x = wrap(self.player.x + angle.cos() * dist, WORLD_W);
        let y = wrap(self.player.y + angle.sin() * dist, WORLD_H);
        let (radius, health) = match kind {
            EnemyKind::Drone => (16.0, 40.0),
            EnemyKind::Turret => (20.0, 60.0),
            EnemyKind::Slinger => (18.0, 50.0),
        };
        let idx = self.spawn(Faction::Enemy, x, y, 0.0, 0.0, radius, 2.0, health);
        self.entities[idx].enemy = kind;
    }

    fn spawn_boss(&mut self) {
        let x = wrap(self.player.x + 600.0, WORLD_W);
        let y = self.player.y;
        let idx = self.spawn(Faction::Boss, x, y, 0.0, 0.0, 60.0, 20.0, 600.0);
        self.entities[idx].enemy = EnemyKind::Turret;
        self.boss_spawned = true;
        self.boss_alive = true;
    }

    pub fn update(&mut self, dt: f32, input: &Input) {
        if self.over || self.won {
            return;
        }
        self.time += dt;
        self.stage_time += dt;
        self.shake = (self.shake - dt * 30.0).max(0.0);

        self.update_player(dt, input);
        self.update_gravity(dt, input);
        self.update_entities(dt);
        self.update_spawning(dt);
        self.resolve_combat();
        self.update_score(dt);

        if self.mode == Mode::Arcade {
            if self.stage == 5 && !self.boss_spawned && self.stage_time > 20.0 {
                self.spawn_boss();
            }
            if self.stage == 5 {
                if self.boss_spawned && !self.boss_alive && self.stage_time > 25.0 {
                    self.won = true;
                }
            } else if self.stage_time > 30.0 {
                self.won = true;
            }
        }
    }

    fn update_player(&mut self, dt: f32, input: &Input) {
        let p = &mut self.player;
        if p.dead {
            return;
        }
        let (mx, my) = normalize(input.move_x, input.move_y);
        p.aim_x = input.aim_x;
        p.aim_y = input.aim_y;
        p.weapon = input.weapon;
        if p.dash_t > 0.0 {
            p.dash_t -= dt;
            p.vx = p.dash_dir_x * 900.0;
            p.vy = p.dash_dir_y * 900.0;
        } else {
            let k = (12.0 * dt).min(1.0);
            p.vx += (mx * 320.0 - p.vx) * k;
            p.vy += (my * 320.0 - p.vy) * k;
        }
        p.x = wrap(p.x + p.vx * dt, WORLD_W);
        p.y = wrap(p.y + p.vy * dt, WORLD_H);
        p.dash_cd = (p.dash_cd - dt).max(0.0);
        p.invuln = (p.invuln - dt).max(0.0);
        p.heat = (p.heat - 18.0 * dt).max(0.0);
        p.fire_t = (p.fire_t - dt).max(0.0);
        if p.overheated {
            p.overheat_t -= dt;
            if p.overheat_t <= 0.0 {
                p.overheated = false;
                p.heat = 0.0;
            }
        }
        if input.dash && p.dash_cd <= 0.0 && p.dash_t <= 0.0 {
            let (dx, dy) = if mx != 0.0 || my != 0.0 { (mx, my) } else { normalize(input.aim_x, input.aim_y) };
            p.dash_dir_x = dx;
            p.dash_dir_y = dy;
            p.dash_t = 0.18;
            p.dash_cd = 2.5;
            p.invuln = 0.18;
            self.events.push(Event::Dash(p.x, p.y));
        }
        let can_fire = !p.overheated && p.fire_t <= 0.0;
        let can_bomb = p.bombs > 0;
        if input.fire && can_fire {
            self.fire();
        }
        if input.bomb && can_bomb {
            self.bomb();
        }
    }

    fn fire(&mut self) {
        let (weapon, upgrades, px, py, ax, ay) = {
            let p = &self.player;
            let (ax, ay) = normalize(p.aim_x, p.aim_y);
            (p.weapon, p.upgrades, p.x, p.y, ax, ay)
        };
        let (damage, rate, heat, speed, range, pierce) = match weapon {
            0 => (10.0 + upgrades[0] as f32 * 4.0, 12.0 + upgrades[0] as f32 * 2.0, 4.0, 900.0, 2.0, 0),
            1 => (6.0 + upgrades[1] as f32 * 3.0, 20.0 + upgrades[1] as f32 * 2.0, 2.5, 520.0, 0.5, 0),
            2 => (9.0 + upgrades[2] as f32 * 4.0, 6.0 + upgrades[2] as f32 * 0.5, 6.0, 1100.0, 2.0, upgrades[2] as i32),
            _ => return,
        };
        if self.player.heat + heat > 100.0 {
            self.player.overheated = true;
            self.player.overheat_t = 1.2;
            self.player.heat = 100.0;
            return;
        }
        let idx = self.spawn(Faction::Projectile, px + ax * 18.0, py + ay * 18.0, ax * speed, ay * speed, 3.0, 0.1, 1.0);
        {
            let e = &mut self.entities[idx];
            e.damage = damage;
            e.weapon = weapon;
            e.pierce = pierce;
            e.life = range;
        }
        self.player.heat += heat;
        self.player.fire_t = 1.0 / rate;
        self.events.push(Event::Fire(weapon, px, py));
    }

    fn bomb(&mut self) {
        let p = &mut self.player;
        p.bombs -= 1;
        self.events.push(Event::Bomb(p.x, p.y));
        self.shake = 18.0;
        let mut i = 0;
        while i < self.entities.len() {
            if self.entities[i].faction == Faction::Meteor {
                let e = self.entities[i];
                self.shatter(i);
                self.add_kill(e.x, e.y, false);
            } else {
                i += 1;
            }
        }
    }

    fn update_gravity(&mut self, dt: f32, input: &Input) {
        let p = &mut self.player;
        if p.dead {
            return;
        }
        let radius = 300.0 + p.upgrades[3] as f32 * 40.0;
        if input.gravity && p.weapon == 3 {
            if p.grabbed == 0 {
                let mut best = 0u64;
                let mut best_d = radius;
                for e in &self.entities {
                    if e.faction != Faction::Meteor {
                        continue;
                    }
                    let d = (wrap_delta(p.x, e.x, WORLD_W).powi(2) + wrap_delta(p.y, e.y, WORLD_H).powi(2)).sqrt();
                    if d < best_d {
                        best_d = d;
                        best = e.id;
                    }
                }
                if best != 0 {
                    p.grabbed = best;
                    self.events.push(Event::Grab(p.x, p.y));
                }
            }
            if p.grabbed != 0 {
                let (ax, ay) = normalize(input.aim_x, input.aim_y);
                if let Some(e) = self.entities.iter_mut().find(|e| e.id == p.grabbed) {
                    e.x = wrap(p.x + ax * 70.0, WORLD_W);
                    e.y = wrap(p.y + ay * 70.0, WORLD_H);
                    e.vx = p.vx;
                    e.vy = p.vy;
                }
            }
        } else if p.grabbed != 0 {
            let (ax, ay) = normalize(input.aim_x, input.aim_y);
            if let Some(e) = self.entities.iter_mut().find(|e| e.id == p.grabbed) {
                e.vx = ax * 1200.0;
                e.vy = ay * 1200.0;
                self.events.push(Event::Sling(e.x, e.y));
            }
            p.grabbed = 0;
        }
        let _ = dt;
    }

    fn update_entities(&mut self, dt: f32) {
        let px = self.player.x;
        let py = self.player.y;
        let mut pending: Vec<(f32, f32, f32, f32, f32)> = Vec::new();
        for e in &mut self.entities {
            match e.faction {
                Faction::Meteor | Faction::Debris => {
                    e.x = wrap(e.x + e.vx * dt, WORLD_W);
                    e.y = wrap(e.y + e.vy * dt, WORLD_H);
                    e.rot += e.rot_speed * dt;
                    e.grace = (e.grace - dt).max(0.0);
                }
                Faction::Projectile => {
                    e.life -= dt;
                    e.x = wrap(e.x + e.vx * dt, WORLD_W);
                    e.y = wrap(e.y + e.vy * dt, WORLD_H);
                }
                Faction::Enemy => {
                    let dx = wrap_delta(e.x, px, WORLD_W);
                    let dy = wrap_delta(e.y, py, WORLD_H);
                    let d = (dx * dx + dy * dy).sqrt().max(1.0);
                    match e.enemy {
                        EnemyKind::Drone => {
                            e.vx += dx / d * 120.0 * dt;
                            e.vy += dy / d * 120.0 * dt;
                            let s = (e.vx * e.vx + e.vy * e.vy).sqrt();
                            if s > 180.0 {
                                e.vx *= 180.0 / s;
                                e.vy *= 180.0 / s;
                            }
                        }
                        EnemyKind::Turret => {
                            e.vx *= 0.9;
                            e.vy *= 0.9;
                        }
                        EnemyKind::Slinger => {
                            let want = 420.0;
                            let k = if d < want { -1.0 } else { 1.0 };
                            e.vx += dx / d * 90.0 * k * dt;
                            e.vy += dy / d * 90.0 * k * dt;
                        }
                    }
                    e.x = wrap(e.x + e.vx * dt, WORLD_W);
                    e.y = wrap(e.y + e.vy * dt, WORLD_H);
                    e.rot += e.rot_speed * dt;
                    e.fire_t -= dt;
                    if e.fire_t <= 0.0 && d < 900.0 {
                        e.fire_t = match e.enemy {
                            EnemyKind::Turret => 1.4,
                            EnemyKind::Slinger => 1.8,
                            _ => 2.4,
                        };
                        pending.push((e.x, e.y, dx / d * 420.0, dy / d * 420.0, 8.0));
                    }
                }
                Faction::Boss => {
                    let dx = wrap_delta(e.x, px, WORLD_W);
                    let dy = wrap_delta(e.y, py, WORLD_H);
                    let d = (dx * dx + dy * dy).sqrt().max(1.0);
                    e.vx += dx / d * 40.0 * dt;
                    e.vy += dy / d * 40.0 * dt;
                    e.x = wrap(e.x + e.vx * dt, WORLD_W);
                    e.y = wrap(e.y + e.vy * dt, WORLD_H);
                    e.rot += e.rot_speed * dt * 0.4;
                    e.fire_t -= dt;
                    if e.fire_t <= 0.0 {
                        e.fire_t = 0.9;
                        for k in 0..3 {
                            let a = (k as f32) * 0.4 - 0.4;
                            let (ax, ay) = normalize(dx / d + a, dy / d - a);
                            pending.push((e.x, e.y, ax * 380.0, ay * 380.0, 12.0));
                        }
                    }
                }
                _ => {}
            }
        }
        self.entities.retain(|e| e.faction != Faction::Projectile || e.life > 0.0);
        for (x, y, vx, vy, dmg) in pending {
            let idx = self.spawn(Faction::Projectile, x, y, vx, vy, 4.0, 0.1, 1.0);
            let pe = &mut self.entities[idx];
            pe.damage = dmg;
            pe.weapon = 9;
            pe.life = 2.0;
        }
    }

    fn update_spawning(&mut self, dt: f32) {
        self.spawn_t -= dt;
        if self.spawn_t <= 0.0 {
            let base = if self.mode == Mode::Survival {
                (0.6 - self.time * 0.003).max(0.15)
            } else {
                (1.0 - self.stage as f32 * 0.1).max(0.3)
            };
            self.spawn_t = base;
            self.spawn_meteor();
            if self.mode == Mode::Survival && self.time > 20.0 && self.rand() < 0.4 {
                self.spawn_enemy(EnemyKind::Drone);
            }
            if self.mode == Mode::Arcade && self.stage >= 2 && self.rand() < 0.35 {
                let kind = if self.rand() < 0.5 { EnemyKind::Drone } else { EnemyKind::Turret };
                self.spawn_enemy(kind);
            }
            if self.mode == Mode::Arcade && self.stage >= 4 && self.rand() < 0.2 {
                self.spawn_enemy(EnemyKind::Slinger);
            }
        }
    }

    fn shatter(&mut self, index: usize) {
        let e = self.entities[index];
        self.entities.remove(index);
        let fragments = 4 + (self.rand() * 3.0) as usize;
        let ore = (1 + (e.mass / 2.0) as u32).min(4);
        self.player.ore += ore;
        self.events.push(Event::Shatter(e.x, e.y, e.kind, e.mass));
        for _ in 0..fragments {
            let angle = self.rand_range(0.0, 6.283);
            let impulse = self.rand_range(40.0, 120.0);
            let fm = (e.mass / fragments as f32).max(0.2);
            let idx = self.spawn(
                Faction::Debris,
                wrap(e.x + angle.cos() * e.radius * 0.5, WORLD_W),
                wrap(e.y + angle.sin() * e.radius * 0.5, WORLD_H),
                e.vx * 0.5 + angle.cos() * impulse,
                e.vy * 0.5 + angle.sin() * impulse,
                (e.radius * 0.6).max(4.0),
                fm,
                10.0 + fm * 5.0,
            );
            self.entities[idx].kind = e.kind;
        }
    }

    fn resolve_combat(&mut self) {
        let mut i = 0;
        while i < self.entities.len() {
            let a = self.entities[i];
            if a.faction == Faction::Projectile {
                let mut j = 0;
                while j < self.entities.len() {
                    let b = self.entities[j];
                    if j != i && b.faction == Faction::Meteor && b.grace <= 0.0 {
                        let dx = wrap_delta(a.x, b.x, WORLD_W);
                        let dy = wrap_delta(a.y, b.y, WORLD_H);
                        if dx * dx + dy * dy <= (a.radius + b.radius).powi(2) {
                            let mult = if a.weapon == 1 {
                                if b.kind == 1 { 2.5 } else if b.kind == 2 { 0.4 } else { 1.0 }
                            } else if a.weapon == 2 {
                                if b.kind == 2 { 2.5 } else if b.kind == 1 { 0.4 } else { 1.0 }
                            } else {
                                1.0
                            };
                            self.entities[j].health -= a.damage * mult;
                            self.entities[i].pierce -= 1;
                            if self.entities[i].pierce < 0 {
                                self.entities[i].life = 0.0;
                            }
                            if self.entities[j].health <= 0.0 {
                                let e = self.entities[j];
                                self.shatter(j);
                                self.add_kill(e.x, e.y, false);
                                if j < i {
                                    i -= 1;
                                }
                                continue;
                            }
                        }
                    }
                    j += 1;
                }
            }
            i += 1;
        }

        // Player and enemy collisions.
        let mut i = 0;
        while i < self.entities.len() {
            let e = self.entities[i];
            let dx = wrap_delta(e.x, self.player.x, WORLD_W);
            let dy = wrap_delta(e.y, self.player.y, WORLD_H);
            let hit = dx * dx + dy * dy <= (e.radius + PLAYER_RADIUS).powi(2);
            if hit {
                match e.faction {
                    Faction::Meteor => {
                        if self.player.invuln <= 0.0 {
                            self.damage_player(e.mass * 1.0);
                        }
                        self.entities.remove(i);
                        self.add_kill(e.x, e.y, false);
                        continue;
                    }
                    Faction::Enemy | Faction::Boss => {
                        if self.player.invuln <= 0.0 {
                            self.damage_player(if e.faction == Faction::Boss { 20.0 } else { 10.0 });
                        }
                    }
                    Faction::Projectile => {
                        if e.weapon == 9 && self.player.invuln <= 0.0 {
                            self.damage_player(e.damage);
                            self.entities.remove(i);
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }

        // Projectile vs enemy/boss.
        let mut i = 0;
        while i < self.entities.len() {
            let a = self.entities[i];
            if a.faction == Faction::Projectile && a.weapon != 9 {
                let mut j = 0;
                while j < self.entities.len() {
                    let b = self.entities[j];
                    if j != i && (b.faction == Faction::Enemy || b.faction == Faction::Boss) {
                        let dx = wrap_delta(a.x, b.x, WORLD_W);
                        let dy = wrap_delta(a.y, b.y, WORLD_H);
                        if dx * dx + dy * dy <= (a.radius + b.radius).powi(2) {
                            self.entities[j].health -= a.damage;
                            self.entities[i].pierce -= 1;
                            if self.entities[i].pierce < 0 {
                                self.entities[i].life = 0.0;
                            }
                            if self.entities[j].health <= 0.0 {
                                let e = self.entities[j];
                                if e.faction == Faction::Boss {
                                    self.boss_alive = false;
                                }
                                self.entities.remove(j);
                                self.add_kill(e.x, e.y, true);
                                if j < i {
                                    i -= 1;
                                }
                                continue;
                            }
                        }
                    }
                    j += 1;
                }
            }
            i += 1;
        }

        self.entities.retain(|e| e.faction != Faction::Projectile || e.life > 0.0);
    }

    fn add_kill(&mut self, x: f32, y: f32, enemy: bool) {
        self.combo = (self.combo + 0.5).min(8.0);
        self.combo_idle = 0.0;
        let base = if enemy { 200.0 } else { 100.0 };
        self.score += (base * self.combo) as u64;
        self.events.push(Event::Kill(x, y));
    }

    fn update_score(&mut self, dt: f32) {
        self.combo_idle += dt;
        if self.combo_idle > 2.0 {
            self.combo = (self.combo - 0.5 * dt).max(1.0);
        }
    }

    fn damage_player(&mut self, amount: f32) {
        if self.player.dead {
            return;
        }
        self.player.health -= amount;
        self.player.invuln = 0.5;
        self.shake = 8.0;
        self.events.push(Event::PlayerHit(amount));
        if self.player.health <= 0.0 {
            self.player.health = 0.0;
            self.player.dead = true;
            self.over = true;
        }
    }
}

impl Player {
    pub fn aim_angle(&self) -> f32 {
        self.aim_y.atan2(self.aim_x)
    }
}

pub struct Input {
    pub move_x: f32,
    pub move_y: f32,
    pub aim_x: f32,
    pub aim_y: f32,
    pub fire: bool,
    pub gravity: bool,
    pub dash: bool,
    pub bomb: bool,
    pub weapon: u8,
    pub dash_dir_x: f32,
    pub dash_dir_y: f32,
}

fn normalize(x: f32, y: f32) -> (f32, f32) {
    let len = (x * x + y * y).sqrt();
    if len < 1e-6 {
        (0.0, 0.0)
    } else {
        (x / len, y / len)
    }
}
