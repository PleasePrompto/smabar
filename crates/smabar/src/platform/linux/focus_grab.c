/* Optional Hyprland layer-surface dismissal, dispatched by GTK's Wayland loop. */
#include <gtk/gtk.h>
#include <gdk/gdkwayland.h>
#include <wayland-client.h>
#include <string.h>
#include "hyprland-focus-grab-v1.h"

#define STATE_KEY "smabar-focus-grab"

typedef struct {
    struct hyprland_focus_grab_manager_v1 *manager;
    struct hyprland_focus_grab_v1 *grab;
    void *context;
    void (*cleared)(void *, uint64_t);
    GDestroyNotify free_context;
    uint64_t generation;
} FocusGrab;

static void release_grab(FocusGrab *state) {
    if (state->grab) {
        hyprland_focus_grab_v1_destroy(state->grab);
        state->grab = NULL;
    }
}

static void free_state(gpointer data) {
    FocusGrab *state = data;
    release_grab(state);
    if (state->manager)
        hyprland_focus_grab_manager_v1_destroy(state->manager);
    state->free_context(state->context);
    g_free(state);
}

static void unmapped(GtkWidget *component, gpointer data) {
    (void)component;
    release_grab(data);
}

static void cleared(void *data, struct hyprland_focus_grab_v1 *grab) {
    (void)grab;
    FocusGrab *state = data;
    release_grab(state);
    state->cleared(state->context, state->generation);
}

static const struct hyprland_focus_grab_v1_listener grab_listener = { cleared };

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)version;
    FocusGrab *state = data;
    if (strcmp(interface, hyprland_focus_grab_manager_v1_interface.name) == 0)
        state->manager = wl_registry_bind(registry, name,
            &hyprland_focus_grab_manager_v1_interface, 1);
}

static void global_remove(void *data, struct wl_registry *registry, uint32_t name) {
    /* The registry is used only during discovery; bound objects remain valid. */
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = { global, global_remove };

/* Always takes ownership of context. Zero means the compositor lacks this protocol. */
int smabar_focus_grab_install(GtkWindow *window, void *context,
                             void (*on_cleared)(void *, uint64_t),
                             GDestroyNotify free_context) {
    FocusGrab *state = g_new0(FocusGrab, 1);
    state->context = context;
    state->cleared = on_cleared;
    state->free_context = free_context;
    g_object_set_data_full(G_OBJECT(window), STATE_KEY, state, free_state);
    g_signal_connect(window, "unmap", G_CALLBACK(unmapped), state);
    struct wl_display *display = gdk_wayland_display_get_wl_display(
        gtk_widget_get_display(GTK_WIDGET(window)));
    /* Discover on a private queue, without re-entering GTK during setup. */
    struct wl_event_queue *queue = wl_display_create_queue(display);
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_proxy_set_queue((struct wl_proxy *)registry, queue);
    wl_registry_add_listener(registry, &registry_listener, state);
    int result = wl_display_roundtrip_queue(display, queue);
    if (state->manager)
        wl_proxy_set_queue((struct wl_proxy *)state->manager, NULL);
    wl_registry_destroy(registry);
    wl_event_queue_destroy(queue);
    return result < 0 ? -1 : state->manager != NULL;
}

int smabar_focus_grab_activate(GtkWindow *window, GtkWindow *bar, uint64_t generation) {
    FocusGrab *state = g_object_get_data(G_OBJECT(window), STATE_KEY);
    if (!state || !state->manager)
        return 0;
    GdkWindow *overlay_window = gtk_widget_get_window(GTK_WIDGET(window));
    GdkWindow *bar_window = gtk_widget_get_window(GTK_WIDGET(bar));
    if (!overlay_window || !bar_window)
        return -1;
    struct wl_surface *overlay_surface = gdk_wayland_window_get_wl_surface(overlay_window);
    struct wl_surface *bar_surface = gdk_wayland_window_get_wl_surface(bar_window);
    if (!overlay_surface || !bar_surface)
        return -1;
    state->generation = generation;
    if (!state->grab) {
        state->grab = hyprland_focus_grab_manager_v1_create_grab(state->manager);
        hyprland_focus_grab_v1_add_listener(state->grab, &grab_listener, state);
        hyprland_focus_grab_v1_add_surface(state->grab, overlay_surface);
        hyprland_focus_grab_v1_add_surface(state->grab, bar_surface);
        hyprland_focus_grab_v1_commit(state->grab);
    }
    return 1;
}

void smabar_focus_grab_release(GtkWindow *window) {
    FocusGrab *state = g_object_get_data(G_OBJECT(window), STATE_KEY);
    if (state)
        release_grab(state);
}
