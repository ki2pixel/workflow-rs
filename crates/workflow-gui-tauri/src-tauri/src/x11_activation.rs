//! Activation X11 d'une fenêtre existante.
//!
//! `gtk_window_present_with_time(GDK_CURRENT_TIME)` est insuffisant sous GNOME :
//! la politique anti-vol de focus peut laisser la fenêtre minimisée et marquer
//! `_NET_WM_STATE_DEMANDS_ATTENTION`. On envoie donc explicitement
//! `_NET_ACTIVE_WINDOW` avec l'indication « pager » (2), comme `wmctrl -a`.

use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ClientMessageData, ClientMessageEvent, ConnectionExt, EventMask, CLIENT_MESSAGE_EVENT,
};
use x11rb::wrapper::ConnectionExt as _;

/// Demande au gestionnaire de fenêtres d'activer `window_id`.
pub fn activate(window_id: u32) -> Result<()> {
    let (connection, screen_num) = x11rb::connect(None).context("connexion X11")?;
    let screen = &connection.setup().roots[screen_num];
    let atom = connection
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .context("requête de l'atome _NET_ACTIVE_WINDOW")?
        .reply()
        .context("réponse de l'atome _NET_ACTIVE_WINDOW")?
        .atom;
    let event = ClientMessageEvent {
        response_type: CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window: window_id,
        type_: atom,
        // data[0] = 2 : activation demandée par un pager, ce qui contourne la
        // politique de vol de focus. data[1] = CurrentTime.
        data: ClientMessageData::from([2u32, 0, 0, 0, 0]),
    };
    connection
        .send_event(
            false,
            screen.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .context("envoi de _NET_ACTIVE_WINDOW")?
        .check()
        .context("accusé de réception de _NET_ACTIVE_WINDOW")?;
    // `sync` garantit que la requête a été traitée avant la fermeture de la
    // connexion X11, contrairement à un simple `flush`.
    connection.sync().context("synchronisation X11")?;
    Ok(())
}
