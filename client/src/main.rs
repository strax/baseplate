#![feature(mem_take)]
#![feature(async_closure)]

use std::borrow::Borrow;
use std::borrow::BorrowMut;
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::mem;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use async_std::{future::timeout, net::UdpSocket, task};
use async_std::net::ToSocketAddrs;
use async_std::sync::Arc;
use bincode;
use bytes::Bytes;
use futures::{executor, Poll};
use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::future::Fuse;
use futures::lock::Mutex;
use futures::pin_mut;
use futures::prelude::*;
use futures::select;
use futures::stream;
use futures::stream::FusedStream;
use futures::task::Context;
use futures_timer::Delay;
use ggez;
use ggez::conf::{NumSamples, WindowSetup};
use ggez::event::{self, EventHandler, EventsLoop};
use ggez::event::winit_event::{Event, KeyboardInput, WindowEvent};
use ggez::graphics;
use ggez::input;
use ggez::input::keyboard::KeyCode;
use ggez::nalgebra as na;
use log::{debug, error, info, trace, warn};

use conn::Conn;
use shared::{handshake::*, hexdump, logging, packet::Packet, proto::*, Result};
use shared::future::retry;

mod conn;

async fn keep_alive(conn: Arc<Conn>) {
    loop {
        Delay::new(Duration::from_secs(5)).await;
        conn.send(Message::Heartbeat).await.unwrap_or_else(|err| warn!("error sending heartbeat: {}", err));
    }
}

struct GameState {
    positions: Vec<(f32, f32)>
}

impl GameState {
    fn new() -> ggez::GameResult<GameState> {
        Ok(GameState { positions: vec![] })
    }
}

fn draw(state: &GameState, ctx: &mut ggez::Context) -> ggez::GameResult {
    graphics::clear(ctx, [1.0, 1.0, 1.0, 1.0].into());

    for pos in state.positions.clone() {
        let circle = graphics::Mesh::new_circle(
            ctx,
            graphics::DrawMode::fill(),
            na::Point2::new(pos.0, pos.1),
            30.0,
            1.0,
            [1.0, 0.0, 0.0, 1.0].into()
        )?;
        graphics::draw(ctx, &circle, (na::Point2::new(50.0, 50.0),))?;
    }

    graphics::present(ctx)?;
    Ok(())
}

fn on_event(event: &Event, ctx: &mut ggez::Context) {
    match event {
        Event::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => event::quit(ctx),
            WindowEvent::KeyboardInput {
                input: KeyboardInput { virtual_keycode: Some(keycode), .. }, ..
            } => match keycode {
                event::KeyCode::Escape => event::quit(ctx),
                _ => {}
            },
            // `CloseRequested` and `KeyboardInput` events won't appear here.
            x => println!("Other window event fired: {:?}", x)
        }
        x => println!("Device event fired: {:?}", x)
    }
}

async fn async_main() -> () {
    logging::setup().unwrap();

    trace!("client starting");

    // Try to create a connection, retrying 5 times
    let conn = Arc::new(retry(5, async || {
        Conn::connect(SocketAddr::from_str("127.0.0.1:12345")?).await
    }).await.expect("unable to connect to the server"));
    info!("connection estabilished");

    task::spawn(keep_alive(conn.clone()));

    let cb = ggez::ContextBuilder::new("baseplate", "strax").window_setup(WindowSetup {
        title: "baseplate".to_owned(),
        samples: NumSamples::Zero,
        vsync: true,
        icon: "".to_owned(),
        srgb: true
    });
    let (ctx, event_loop) = &mut cb.build().unwrap();
    let state = &mut GameState::new().unwrap();

    let (tx, mut rx) = mpsc::unbounded();

    let next_message = Fuse::terminated();
    let input_tick = Fuse::terminated();
    pin_mut!(next_message, input_tick);
    next_message.set(conn.next_message().fuse());
    input_tick.set(Delay::new(Duration::from_millis(16)).fuse());


    while ctx.continuing {
        ctx.timer_context.tick();
        event_loop.poll_events(|event| tx.unbounded_send(event).unwrap());

        select! {
            event = rx.select_next_some() => {
                ctx.process_event(&event);
                on_event(&event, ctx);
            },
            () = input_tick => {
                handle_movement(ctx, &conn).await;
                input_tick.set(Delay::new(Duration::from_millis(16)).fuse());
            }
            msg = next_message => {
                match msg {
                    Message::Refresh(positions) => {
                        state.positions = positions;
                        draw(state, ctx).unwrap();
                    },
                    _ => {}
                }
                next_message.set(conn.next_message().fuse());
            }
        }
    }

    conn.send(Message::Disconnect).await;
}

async fn handle_movement(ctx: &mut ggez::Context, conn: &Conn) {
    if input::keyboard::is_key_pressed(ctx, KeyCode::Right) {
        conn.send(Message::Move { dx: 1.0, dy: 0.0 }).await;
    }
    if input::keyboard::is_key_pressed(ctx, KeyCode::Left) {
        conn.send(Message::Move { dx: -1.0, dy: 0.0 }).await;
    }
    if input::keyboard::is_key_pressed(ctx, KeyCode::Down) {
        conn.send(Message::Move { dx: 0.0, dy: 1.0 }).await;
    }
    if input::keyboard::is_key_pressed(ctx, KeyCode::Up) {
        conn.send(Message::Move { dx: 0.0, dy: -1.0 }).await;
    }
}

fn main() {
    executor::block_on(async_main());
}
