//! A transparent layer surface owns the reservation independently of bar stacking.

use std::cell::RefCell;

use anyhow::Context;
use gtk::prelude::{GtkWindowExt, WidgetExt};
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use super::strut::DockEdge;

thread_local! {
    // GTK handles belong to the main thread, like every caller of this module.
    static PROXY: RefCell<Option<gtk::Window>> = const { RefCell::new(None) };
}

pub(super) fn height() -> i32 {
    PROXY.with_borrow(|proxy| proxy.as_ref().map_or(0, LayerShell::exclusive_zone))
}

pub(super) fn apply(
    bar: &gtk::ApplicationWindow,
    edge: Option<DockEdge>,
    zone: i32,
) -> anyhow::Result<()> {
    let monitor = bar
        .monitor()
        .context("Wayland bar has no monitor for its reservation")?;
    PROXY.with_borrow_mut(|slot| {
        let old = slot
            .as_ref()
            .map(|proxy| (proxy.is_anchor(Edge::Top), proxy.exclusive_zone()));
        if zone > 0 && slot.is_none() {
            let proxy = gtk::Window::new(gtk::WindowType::Toplevel);
            proxy.set_decorated(false);
            proxy.set_app_paintable(true);
            proxy.set_visual(bar.visual().as_ref());
            proxy.set_size_request(1, 1);
            proxy.init_layer_shell();
            proxy.set_namespace("smabar-reservation");
            proxy.set_layer(Layer::Top);
            proxy.set_keyboard_mode(KeyboardMode::None);
            proxy.connect_draw(|_, context| {
                context.set_operator(gtk::cairo::Operator::Clear);
                if let Err(error) = context.paint() {
                    tracing::error!(%error, "failed to clear the Wayland reservation surface");
                }
                gtk::glib::Propagation::Stop
            });
            bar.connect_destroy(|_| {
                PROXY.with_borrow_mut(|slot| {
                    if let Some(proxy) = slot.take() {
                        proxy.close();
                    }
                });
            });
            *slot = Some(proxy);
        }
        if let Some(proxy) = slot.as_ref() {
            proxy.set_exclusive_zone(zone);
            if zone > 0 {
                proxy.set_monitor(&monitor);
                proxy.set_anchor(Edge::Top, edge == Some(DockEdge::Top));
                proxy.set_anchor(Edge::Bottom, edge == Some(DockEdge::Bottom));
                proxy.show();
                proxy.input_shape_combine_region(Some(&gtk::cairo::Region::create()));
                super::layer::commit(proxy);
            } else {
                proxy.hide();
            }
        }
        // A zero-zone bar avoids all panels, including its own proxy. Move it
        // back into only its own reserved strip; keep other panels untouched.
        for (anchor, top) in [(Edge::Top, true), (Edge::Bottom, false)] {
            let previous = old
                .filter(|(at_top, _)| *at_top == top)
                .map_or(0, |(_, h)| h);
            let next = if edge == Some(if top { DockEdge::Top } else { DockEdge::Bottom }) {
                zone
            } else {
                0
            };
            bar.set_layer_shell_margin(
                anchor,
                bar.layer_shell_margin(anchor)
                    .saturating_add(previous)
                    .saturating_sub(next),
            );
        }
        if zone > 0 || bar.exclusive_zone() >= 0 {
            bar.set_exclusive_zone(0);
        }
        super::layer::commit(bar);
    });
    Ok(())
}
