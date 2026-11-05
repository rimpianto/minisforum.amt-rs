//! KVM window: winit 0.29 + softbuffer 0.3, fed by the RFB session running in
//! a separate tokio thread. Frames arrive through a channel, keyboard and mouse
//! go back through another one.

use anyhow::Result;
use std::num::NonZeroU32;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use winit::event::{ElementState, Event, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

/// A decoded frame handed from the network thread to the UI.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>, // RGBA
}

/// Input events the UI sends back to the RFB thread.
pub enum Input {
    /// A whole keystroke: the loop sends down, a short hold, then up.
    Key { keysym: u32 },
    /// A modifier goes down and stays down until it is released, otherwise
    /// ctrl+c and ctrl+alt+f2 are impossible.
    Modifier { keysym: u32, down: bool },
    Pointer { x: u16, y: u16, buttons: u8 },
}

/// Modifiers we hold down for the host while the user keeps them pressed.
fn modifier_keysym(key: &winit::keyboard::Key) -> Option<u32> {
    use winit::keyboard::{Key, NamedKey};
    match key {
        Key::Named(NamedKey::Shift) => Some(0xFFE1),
        Key::Named(NamedKey::Control) => Some(0xFFE3),
        Key::Named(NamedKey::Alt) => Some(0xFFE9),
        Key::Named(NamedKey::Super) => Some(0xFFEB),
        _ => None,
    }
}

/// Map a winit key to an RFB keysym.
///
/// The physical key is preferred: with Option (Alt) held, macOS reports a
/// different logical key and the function keys stop looking like themselves,
/// which is exactly how ctrl+alt+f2 was lost. Letters and digits fall back to
/// the logical character, and Shift stays a held modifier on the host, so a
/// shifted letter still comes out shifted.
pub fn key_to_keysym(
    physical: &winit::keyboard::PhysicalKey,
    logical: &winit::keyboard::Key,
) -> Option<u32> {
    use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

    if let PhysicalKey::Code(code) = physical {
        let mapped = match code {
            KeyCode::Enter | KeyCode::NumpadEnter => Some(0xFF0D),
            KeyCode::Tab => Some(0xFF09),
            KeyCode::Escape => Some(0xFF1B),
            KeyCode::Backspace => Some(0xFF08),
            KeyCode::Space => Some(0x20),
            KeyCode::ArrowUp => Some(0xFF52),
            KeyCode::ArrowDown => Some(0xFF54),
            KeyCode::ArrowLeft => Some(0xFF51),
            KeyCode::ArrowRight => Some(0xFF53),
            KeyCode::Delete => Some(0xFFFF),
            KeyCode::Home => Some(0xFF50),
            KeyCode::End => Some(0xFF57),
            KeyCode::PageUp => Some(0xFF55),
            KeyCode::PageDown => Some(0xFF56),
            KeyCode::Insert => Some(0xFF63),
            KeyCode::F1 => Some(0xFFBE),
            KeyCode::F2 => Some(0xFFBF),
            KeyCode::F3 => Some(0xFFC0),
            KeyCode::F4 => Some(0xFFC1),
            KeyCode::F5 => Some(0xFFC2),
            KeyCode::F6 => Some(0xFFC3),
            KeyCode::F7 => Some(0xFFC4),
            KeyCode::F8 => Some(0xFFC5),
            KeyCode::F9 => Some(0xFFC6),
            KeyCode::F10 => Some(0xFFC7),
            KeyCode::F11 => Some(0xFFC8),
            KeyCode::F12 => Some(0xFFC9),
            _ => None,
        };
        if mapped.is_some() {
            return mapped;
        }
    }

    match logical {
        Key::Character(s) => s.chars().next().map(|c| c as u32),
        Key::Named(NamedKey::Space) => Some(0x20),
        Key::Named(NamedKey::Enter) => Some(0xFF0D),
        Key::Named(NamedKey::Tab) => Some(0xFF09),
        Key::Named(NamedKey::Escape) => Some(0xFF1B),
        Key::Named(NamedKey::Backspace) => Some(0xFF08),
        _ => None,
    }
}

pub struct Gui {
    pub frames: Receiver<Frame>,
    pub input: Sender<Input>,
    pub state: Arc<Mutex<(usize, usize)>>,
}

/// Run the window loop on the calling thread; returns when the window closes.
pub fn run_gui(gui: Gui) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let (mut width, mut height) = *gui.state.lock().unwrap();

    // Declared in this order so the surface (dropped first) goes before the
    // context it borrows the display from.
    let mut _context: Option<softbuffer::Context<Arc<Window>>> = None;
    let mut surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>> = None;
    let mut window: Option<Arc<Window>> = None;
    let mut last: Option<Frame> = None;
    let mut redraws = 0u64;
    let mut force_draw = true;
    let mut win_w = width as u32;
    let mut win_h = height as u32;
    // The pointer position stays put between events: a button event that sends
    // (0,0) teleports the host cursor to the top-left corner.
    let mut pos = (0u16, 0u16);
    let mut held_mods: Vec<u32> = Vec::new();
    let mut fb_dims = (0usize, 0usize);
    let mut mask = 0u8;
    // Manual zoom and pan. zoom_idx walks the ladder (fit, then 1:1, 2:1,
    // 3:1, 4:1, 6:1); pan is the picture offset in window pixels and only
    // matters while the picture is bigger than the window. ctrl+wheel or
    // ctrl+plus/minus change the zoom, ctrl+0 resets both; ctrl+left-drag
    // pans instead of reaching the host.
    let mut zoom_idx = 0usize;
    let mut pan = (0i32, 0i32);
    let mut cursor = (0f64, 0f64);
    // While set: (cursor at drag start, pan at drag start).
    let mut pan_drag: Option<((f64, f64), (i32, i32))> = None;

    event_loop.run(move |event, elwt| {
        // Poll the frame channel ~60 times a second. Without this the redraw
        // request below feeds itself and the loop spins at tens of thousands
        // of frames per second, burning a core for nothing.
        elwt.set_control_flow(ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(16),
        ));

        match event {
            Event::Resumed => {
                let win = Arc::new(
                    WindowBuilder::new()
                        .with_title(format!(
                            "amt-rs {} ({}) — KVM",
                            crate::VERSION,
                            crate::GIT_HASH
                        ))
                        .with_inner_size(winit::dpi::PhysicalSize::new(
                            width as u32,
                            height as u32,
                        ))
                        .build(elwt)
                        .expect("cannot create window"),
                );

                // The framebuffer is 16:9; a window clamped by a smaller screen
                // keeps the picture letterboxed, so ask for the largest size
                // that fits the monitor at the same ratio.
                if let Some(mon) = win.current_monitor() {
                    let scale = mon.scale_factor();
                    let (mw, mh) = (mon.size().width as f64, mon.size().height as f64);
                    let (avail_w, avail_h) = (mw * 0.92, mh * 0.92);
                    let factor = (avail_w / width as f64)
                        .min(avail_h / height as f64)
                        .min(1.0);
                    if factor < 0.999 {
                        let _ = win.request_inner_size(winit::dpi::PhysicalSize::new(
                            (width as f64 * factor) as u32,
                            (height as f64 * factor) as u32,
                        ));
                    }
                    let _ = scale;
                }
                let ctx = softbuffer::Context::new(win.clone()).expect("softbuffer context");
                let mut surf =
                    softbuffer::Surface::new(&ctx, win.clone()).expect("softbuffer surface");
                let _ = surf.resize(
                    NonZeroU32::new(width as u32).unwrap_or(NonZeroU32::new(1).unwrap()),
                    NonZeroU32::new(height as u32).unwrap_or(NonZeroU32::new(1).unwrap()),
                );
                if std::env::var("AMT_TRACE").is_ok() {
                    eprintln!("[gui] window created {}x{}", width, height);
                }
                win_w = win.inner_size().width.max(1);
                win_h = win.inner_size().height.max(1);
                // Paint it black straight away: until the first frame arrives
                // the window would show the platform's white background.
                if let Ok(mut buf) = surf.buffer_mut() {
                    for px in buf.iter_mut() {
                        *px = 0;
                    }
                    let _ = buf.present();
                }
                win.request_redraw();
                window = Some(win);
                _context = Some(ctx);
                surface = Some(surf);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if let Some(win) = window.as_ref() {
                    win.request_redraw();
                }
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::KeyboardInput { event, .. } => {
                    if let Some(k) = modifier_keysym(&event.logical_key) {
                        let down = event.state == ElementState::Pressed;
                        if down != held_mods.contains(&k) {
                            if down {
                                held_mods.push(k);
                            } else {
                                held_mods.retain(|m| *m != k);
                            }
                            let _ = gui.input.send(Input::Modifier { keysym: k, down });
                        }
                    } else if event.state == ElementState::Pressed {
                        // With ctrl held, plus/minus/zero zoom the local
                        // picture instead of reaching the host console.
                        // Matched on PHYSICAL keys: logical characters are
                        // layout-dependent (on an Italian layout VK_OEM_MINUS
                        // types "'", on others '-'), while the physical key
                        // is the same key everywhere — this is what makes the
                        // shortcut work the same on Windows and macOS.
                        use winit::keyboard::{KeyCode, PhysicalKey};
                        if held_mods.contains(&0xFFE3) {
                            let (fw, fh) = {
                                let st = gui.state.lock().unwrap();
                                (st.0, st.1)
                            };
                            let ladder = zoom_ladder(win_w, win_h, fw, fh);
                            let zoomed = match event.physical_key {
                                PhysicalKey::Code(KeyCode::Equal)
                                | PhysicalKey::Code(KeyCode::NumpadAdd) => {
                                    zoom_idx = ladder_step(zoom_idx, 1, ladder.len());
                                    Some(ladder[zoom_idx])
                                }
                                PhysicalKey::Code(KeyCode::Minus)
                                | PhysicalKey::Code(KeyCode::NumpadSubtract) => {
                                    zoom_idx = ladder_step(zoom_idx, -1, ladder.len());
                                    Some(ladder[zoom_idx])
                                }
                                PhysicalKey::Code(KeyCode::Digit0)
                                | PhysicalKey::Code(KeyCode::Numpad0) => {
                                    zoom_idx = 0;
                                    pan = (0, 0);
                                    Some(ladder[zoom_idx])
                                }
                                _ => None,
                            };
                            if let Some(scale) = zoomed {
                                force_draw = true;
                                if std::env::var("AMT_TRACE").is_ok() {
                                    eprintln!(
                                        "[gui] zoom {}x{} (idx {}, scale {:.2}, pan {:?})",
                                        fw, fh, zoom_idx, scale, pan
                                    );
                                }
                                if let Some(win) = window.as_ref() {
                                    win.request_redraw();
                                }
                                return;
                            }
                        }
                        if let Some(k) = key_to_keysym(&event.physical_key, &event.logical_key) {
                            let _ = gui.input.send(Input::Key { keysym: k });
                        }
                    }
                }
                WindowEvent::Focused(false) => {
                    // Do not leave ctrl stuck down on the host if the window
                    // loses focus mid-chord.
                    for k in held_mods.drain(..) {
                        let _ = gui.input.send(Input::Modifier { keysym: k, down: false });
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let (fw, fh) = {
                        let st = gui.state.lock().unwrap();
                        (st.0.max(1), st.1.max(1))
                    };
                    cursor = (position.x, position.y);
                    // ctrl+left-drag pans the zoomed picture instead of
                    // reaching the host.
                    if let Some(((sx, sy), (pan_sx, pan_sy))) = pan_drag {
                        let ladder = zoom_ladder(win_w, win_h, fw, fh);
                        let scale = ladder[zoom_idx.min(ladder.len() - 1)];
                        let want = (
                            pan_sx + (position.x - sx) as i32,
                            pan_sy + (position.y - sy) as i32,
                        );
                        let (_dw, _dh, ox, oy) =
                            draw_geometry(win_w, win_h, fw, fh, scale, want.0, want.1);
                        if (ox, oy) != pan {
                            pan = (ox, oy);
                            force_draw = true;
                            if let Some(win) = window.as_ref() {
                                win.request_redraw();
                            }
                        }
                        return;
                    }
                    // Map through the rectangle the frame is actually drawn
                    // in, so the pointer stays honest at any zoom.
                    let ladder = zoom_ladder(win_w, win_h, fw, fh);
                    let scale = ladder[zoom_idx.min(ladder.len() - 1)];
                    let (hx, hy) =
                        pointer_to_host(position.x, position.y, win_w, win_h, fw, fh, scale, pan.0, pan.1);
                    pos.0 = hx;
                    pos.1 = hy;
                    let _ = gui.input.send(Input::Pointer {
                        x: pos.0,
                        y: pos.1,
                        buttons: mask,
                    });
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let up = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y > 0.0,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y > 0.0,
                    };
                    // ctrl+wheel zooms the local picture instead of scrolling
                    // the remote console.
                    if held_mods.contains(&0xFFE3) {
                        let (fw, fh) = {
                            let st = gui.state.lock().unwrap();
                            (st.0, st.1)
                        };
                        let ladder = zoom_ladder(win_w, win_h, fw, fh);
                        zoom_idx = ladder_step(zoom_idx, if up { 1 } else { -1 }, ladder.len());
                        force_draw = true;
                        if std::env::var("AMT_TRACE").is_ok() {
                            eprintln!(
                                "[gui] zoom idx {} scale {:.2} pan {:?}",
                                zoom_idx, ladder[zoom_idx], pan
                            );
                        }
                        if let Some(win) = window.as_ref() {
                            win.request_redraw();
                        }
                        return;
                    }
                    // RFB has no wheel: buttons 4 and 5 are one click each.
                    let bit = if up { 8u8 } else { 16u8 };
                    for buttons in [mask | bit, mask] {
                        let _ = gui.input.send(Input::Pointer {
                            x: pos.0,
                            y: pos.1,
                            buttons,
                        });
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    // ctrl+left-drag pans; the drag never reaches the host.
                    if button == MouseButton::Left && held_mods.contains(&0xFFE3) {
                        match state {
                            ElementState::Pressed => pan_drag = Some((cursor, pan)),
                            ElementState::Released => {
                                pan_drag = None;
                                // Refresh the host pointer where we stand now.
                                let (fw, fh) = {
                                    let st = gui.state.lock().unwrap();
                                    (st.0.max(1), st.1.max(1))
                                };
                                let ladder = zoom_ladder(win_w, win_h, fw, fh);
                                let scale = ladder[zoom_idx.min(ladder.len() - 1)];
                                let (hx, hy) = pointer_to_host(
                                    cursor.0,
                                    cursor.1,
                                    win_w,
                                    win_h,
                                    fw,
                                    fh,
                                    scale,
                                    pan.0,
                                    pan.1,
                                );
                                pos.0 = hx;
                                pos.1 = hy;
                                let _ = gui.input.send(Input::Pointer {
                                    x: hx,
                                    y: hy,
                                    buttons: mask,
                                });
                            }
                        }
                        return;
                    }
                    let bit = match button {
                        MouseButton::Left => 1u8,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 4,
                        _ => 0,
                    };
                    if state == ElementState::Pressed {
                        mask |= bit;
                    } else {
                        mask &= !bit;
                    }
                    let _ = gui.input.send(Input::Pointer {
                        x: pos.0,
                        y: pos.1,
                        buttons: mask,
                    });
                }
                WindowEvent::RedrawRequested => {
                    let mut dirty = false;
                    while let Ok(frame) = gui.frames.try_recv() {
                        if (frame.width, frame.height) != fb_dims {
                            // POST and the BIOS run at a different resolution:
                            // follow it instead of scaling a small screen up
                            // into a big window. The zoom ladder and the pan
                            // belong to the old picture: drop both.
                            fb_dims = (frame.width, frame.height);
                            zoom_idx = 0;
                            pan = (0, 0);
                            if let Some(win) = window.as_ref() {
                                if let Some(mon) = win.current_monitor() {
                                    let (mw, mh) = (mon.size().width as f64, mon.size().height as f64);
                                    // Integer scaling only: a fractional
                                    // nearest-neighbour upscale makes text
                                    // ragged, which is what looked wrong in
                                    // the setup screen.
                                    let (aw, ah) = (mw * 0.92, mh * 0.92);
                                    let fits = |k: f64| {
                                        frame.width as f64 * k <= aw
                                            && frame.height as f64 * k <= ah
                                    };
                                    let factor = if fits(2.0) {
                                        2.0
                                    } else if fits(1.0) {
                                        1.0
                                    } else {
                                        (aw / frame.width as f64).min(ah / frame.height as f64)
                                    };
                                    let _ = win.request_inner_size(
                                        winit::dpi::PhysicalSize::new(
                                            (frame.width as f64 * factor) as u32,
                                            (frame.height as f64 * factor) as u32,
                                        ),
                                    );
                                }
                                win.request_redraw();
                            }
                        }
                        width = frame.width;
                        height = frame.height;
                        {
                            let mut st = gui.state.lock().unwrap();
                            *st = (frame.width, frame.height);
                        }
                        last = Some(frame);
                        dirty = true;
                    }

                    if !dirty && !force_draw {
                        // Nothing new: do not rescale a full frame 60 times a
                        // second. Exposure and resizes set force_draw.
                        return;
                    }

                    if let (Some(surf), Some(frame)) = (surface.as_mut(), last.as_ref()) {
                        let buf_w = NonZeroU32::new(win_w.max(1)).unwrap_or(NonZeroU32::new(1).unwrap());
                        let buf_h = NonZeroU32::new(win_h.max(1)).unwrap_or(NonZeroU32::new(1).unwrap());
                        if surf.resize(buf_w, buf_h).is_ok() {
                            if let Ok(mut buf) = surf.buffer_mut() {
                                // Draw the framebuffer at the ladder scale
                                // with the current pan: aspect ratio kept,
                                // overflow cropped, slack shows as black
                                // bars.
                                let (fw, fh) = (frame.width.max(1), frame.height.max(1));
                                let (bw, bh) = (buf_w.get() as i64, buf_h.get() as i64);
                                let ladder = zoom_ladder(buf_w.get(), buf_h.get(), fw, fh);
                                let scale = ladder[zoom_idx.min(ladder.len() - 1)];
                                let (dw, dh, g_ox, g_oy) =
                                    draw_geometry(buf_w.get(), buf_h.get(), fw, fh, scale, pan.0, pan.1);
                                let (dw, dh) = (dw as i64, dh as i64);
                                let (g_ox, g_oy) = (g_ox as i64, g_oy as i64);

                                let black = 0u32;
                                for px in buf.iter_mut() {
                                    *px = black;
                                }
                                // Only the visible part of the (possibly
                                // cropped) picture is drawn: window rows and
                                // columns that fall inside the frame rect.
                                // Signed offsets: zoomed past the window, the
                                // frame rect starts before (0, 0).
                                let y0 = g_oy.max(0);
                                let y1 = (g_oy + dh).min(bh).max(0);
                                let x0 = g_ox.max(0);
                                let x1 = (g_ox + dw).min(bw).max(0);
                                for by in y0..y1 {
                                    let sy = ((by - g_oy) * fh as i64 / dh) as usize;
                                    let row = sy * fw * 4;
                                    let out_row = by as usize * bw as usize;
                                    for bx in x0..x1 {
                                        let sx = ((bx - g_ox) * fw as i64 / dw) as usize;
                                        let p = &frame.pixels[row + sx * 4..row + sx * 4 + 4];
                                        buf[out_row + bx as usize] = ((p[0] as u32) << 16)
                                            | ((p[1] as u32) << 8)
                                            | p[2] as u32;
                                    }
                                }
                                let _ = buf.present();
                                dirty = false;
                                force_draw = false;
                            }
                        }
                    }

                    if std::env::var("AMT_TRACE").is_ok() {
                        redraws += 1;
                        if redraws % 30 == 0 {
                            eprintln!(
                                "[gui] redraw {redraws}, frame {}x{}, window {win_w}x{win_h}, dirty {dirty}",
                                last.as_ref().map(|f| f.width).unwrap_or(0),
                                last.as_ref().map(|f| f.height).unwrap_or(0)
                            );
                        }
                    }
                    // The next redraw comes from the timer, not from here.
                }
                WindowEvent::Occluded(false) => {
                    force_draw = true;
                    if let Some(win) = window.as_ref() {
                        win.request_redraw();
                    }
                }
                WindowEvent::Resized(size) => {
                    force_draw = true;
                    win_w = size.width.max(1);
                    win_h = size.height.max(1);
                    if let Some(surf) = surface.as_mut() {
                        let _ = surf.resize(
                            NonZeroU32::new(win_w).unwrap_or(NonZeroU32::new(1).unwrap()),
                            NonZeroU32::new(win_h).unwrap_or(NonZeroU32::new(1).unwrap()),
                        );
                    }
                    if let Some(win) = window.as_ref() {
                        win.request_redraw();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    })?;

    Ok(())
}

/// Build the channels and the GUI state; the caller threads `run_gui`.
pub fn channels() -> (Sender<Frame>, Receiver<Input>, Gui) {
    let (tx_frame, rx_frame) = channel();
    let (tx_input, rx_input) = channel();
    (
        tx_frame,
        rx_input,
        Gui {
            frames: rx_frame,
            input: tx_input,
            state: Arc::new(Mutex::new((1920, 1080))),
        },
    )
}

/// The zoom ladder: the plain fit-to-window scale first, then integer
/// multiples of the framebuffer pixel. Integer scales keep nearest-neighbour
/// text crisp at every step — fractional scales shred thin console fonts.
fn zoom_ladder(buf_w: u32, buf_h: u32, fw: usize, fh: usize) -> Vec<f32> {
    let (fw, fh) = (fw.max(1), fh.max(1));
    let fit = (buf_w as f32 / fw as f32).min(buf_h as f32 / fh as f32);
    let mut v = vec![fit, 1.0, 2.0, 3.0, 4.0, 6.0];
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    v
}

/// Move one step along the ladder, clamped at both ends.
fn ladder_step(idx: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (idx as i64 + delta as i64).clamp(0, len as i64 - 1) as usize
}

/// Keep the pan offset inside what the drawn rectangle allows: a picture
/// bigger than the window can slide up to its edges, a smaller one is
/// centred no matter what was asked.
fn clamp_pan(pan_x: i64, pan_y: i64, bw: i64, bh: i64, dw: i64, dh: i64) -> (i32, i32) {
    let x = if dw >= bw {
        pan_x.clamp(bw - dw, 0)
    } else {
        (bw - dw) / 2
    };
    let y = if dh >= bh {
        pan_y.clamp(bh - dh, 0)
    } else {
        (bh - dh) / 2
    };
    (x as i32, y as i32)
}

/// Where the frame lands inside the window at the given scale and pan,
/// keeping the aspect ratio: `(draw_w, draw_h, off_x, off_y)` in window
/// pixels. Offsets come from the clamped pan, so the drawn rect is always
/// where the user left it — centred when the pan is untouched.
fn draw_geometry(
    buf_w: u32,
    buf_h: u32,
    fw: usize,
    fh: usize,
    scale: f32,
    pan_x: i32,
    pan_y: i32,
) -> (u32, u32, i32, i32) {
    let (fw, fh) = (fw.max(1), fh.max(1));
    let s = scale.max(0.001);
    let dw = ((fw as f32 * s) as u32).clamp(1, u32::MAX);
    let dh = ((fh as f32 * s) as u32).clamp(1, u32::MAX);
    let (ox, oy) = clamp_pan(
        pan_x as i64,
        pan_y as i64,
        buf_w as i64,
        buf_h as i64,
        dw as i64,
        dh as i64,
    );
    (dw, dh, ox, oy)
}

/// Map a position in window pixels to absolute RFB pointer coordinates
/// (0..=65535 on both axes) through the rectangle the frame is drawn in.
/// Points in the black bars clamp to the nearest frame edge instead of
/// teleporting the host cursor across the screen.
fn pointer_to_host(
    x: f64,
    y: f64,
    buf_w: u32,
    buf_h: u32,
    fw: usize,
    fh: usize,
    scale: f32,
    pan_x: i32,
    pan_y: i32,
) -> (u16, u16) {
    let (_dw, _dh, ox, oy) = draw_geometry(buf_w, buf_h, fw, fh, scale, pan_x, pan_y);
    let (fw, fh) = (fw.max(1) as f64, fh.max(1) as f64);
    let s = scale.max(0.001) as f64;
    let fx = ((x - ox as f64) / s).clamp(0.0, fw);
    let fy = ((y - oy as f64) / s).clamp(0.0, fh);
    ((fx / fw * 65535.0) as u16, (fy / fh * 65535.0) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_ladder_is_fit_then_integer_multiples() {
        // Fit is 0.5: the ladder keeps it and adds the integer scales.
        let ladder = zoom_ladder(1000, 500, 2000, 1000);
        assert_eq!(ladder, vec![0.5, 1.0, 2.0, 3.0, 4.0, 6.0]);
    }

    #[test]
    fn zoom_ladder_dedupes_when_fit_is_already_integer() {
        let ladder = zoom_ladder(1000, 500, 1000, 500);
        assert_eq!(ladder, vec![1.0, 2.0, 3.0, 4.0, 6.0]);
    }

    #[test]
    fn ladder_step_clamps_at_both_ends() {
        assert_eq!(ladder_step(0, -1, 6), 0);
        assert_eq!(ladder_step(0, 1, 6), 1);
        assert_eq!(ladder_step(5, 1, 6), 5);
        assert_eq!(ladder_step(2, -3, 6), 0);
        assert_eq!(ladder_step(0, 1, 0), 0, "an empty ladder must not panic");
    }

    #[test]
    fn clamp_pan_lets_a_zoomed_picture_slide_to_its_edges() {
        // Picture twice the window: pan range is [-1000..0] x [-500..0].
        assert_eq!(clamp_pan(0, 0, 1000, 500, 2000, 1000), (0, 0));
        assert_eq!(clamp_pan(-1000, -500, 1000, 500, 2000, 1000), (-1000, -500));
        assert_eq!(
            clamp_pan(-2000, 500, 1000, 500, 2000, 1000),
            (-1000, 0),
            "overshoot clamps to the nearest edge"
        );
    }

    #[test]
    fn clamp_pan_centers_a_picture_smaller_than_the_window() {
        assert_eq!(clamp_pan(-999, 999, 1000, 500, 500, 250), (250, 125));
    }

    #[test]
    fn draw_geometry_honours_the_pan() {
        // Zoomed picture, panned hard left: the offset survives.
        let (dw, dh, ox, oy) = draw_geometry(1000, 500, 1000, 500, 2.0, -1000, -500);
        assert_eq!((dw, dh, ox, oy), (2000, 1000, -1000, -500));
        // Fit-sized picture: always centred.
        let (dw, dh, ox, oy) = draw_geometry(1000, 500, 2000, 1000, 0.5, -500, -250);
        assert_eq!((dw, dh, ox, oy), (1000, 500, 0, 0));
    }

    #[test]
    fn pointer_maps_through_the_zoom_geometry() {
        // Exact fit at 0.5x: the frame fills the window, so the window centre
        // (500, 250) is the frame centre, and (250, 125) is a quarter of the
        // way across.
        let (hx, hy) = pointer_to_host(500.0, 250.0, 1000, 500, 2000, 1000, 0.5, 0, 0);
        assert!(
            (hx as i32 - 32767).abs() <= 1 && (hy as i32 - 32767).abs() <= 1,
            "got ({hx}, {hy})"
        );
        let (hx, hy) = pointer_to_host(250.0, 125.0, 1000, 500, 2000, 1000, 0.5, 0, 0);
        assert!(
            (hx as i32 - 16383).abs() <= 1 && (hy as i32 - 16383).abs() <= 1,
            "got ({hx}, {hy})"
        );
    }

    #[test]
    fn pointer_clamps_inside_the_black_bars() {
        // Letterboxed: frame spans the full width, black bars top and bottom.
        // Window (100, 100) is above the picture: x is a tenth of the way in,
        // y clamps to the top edge.
        let (hx, hy) = pointer_to_host(100.0, 100.0, 1000, 1000, 2000, 1000, 0.5, 0, 0);
        assert!((hx as i32 - 6553).abs() <= 1, "got ({hx}, {hy})");
        assert_eq!(hy, 0);
    }

    #[test]
    fn pointer_follows_the_pan() {
        // 2x on a half-size window, dead-centre pan (-500, -250): the
        // picture is centred, so the window centre is still the frame centre.
        let (hx, hy) = pointer_to_host(500.0, 250.0, 1000, 500, 1000, 500, 2.0, -500, -250);
        assert!(
            (hx as i32 - 32767).abs() <= 1 && (hy as i32 - 32767).abs() <= 1,
            "got ({hx}, {hy})"
        );
        // Panned to the far left edge (-1000, -500): the window's top-left
        // corner now shows the frame's centre.
        let (hx, hy) = pointer_to_host(0.0, 0.0, 1000, 500, 1000, 500, 2.0, -1000, -500);
        assert!(
            (hx as i32 - 32767).abs() <= 1 && (hy as i32 - 32767).abs() <= 1,
            "got ({hx}, {hy})"
        );
        // Same far-left pan, window centre: three quarters into the frame.
        let (hx, _hy) = pointer_to_host(500.0, 250.0, 1000, 500, 1000, 500, 2.0, -1000, -500);
        assert!(
            (hx as i32 - 49151).abs() <= 1,
            "got {hx}"
        );
    }
}
