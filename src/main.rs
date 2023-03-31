// Most of the code used here is borrowed from
// https://github.com/diwic/dbus-rs/
// https://github.com/jac0b-w/AlbumPaper/tree/master
// https://github.com/l1na-forever/mpris-notifier/tree/mainline

use {
    dbus::{
        arg::messageitem::MessageItem, blocking::Connection, channel::MatchingReceiver,
        message::MatchRule, Message,
    },
    giftwrap::Wrap,
    mpris::PlayerFinder,
    reqwest::blocking::get,
    std::{io::Cursor, time::Duration},
};

mod image_modifier;

#[derive(Debug, Wrap)]
enum Error {
    Reqwest(reqwest::Error),
    String(String),
    DBus(mpris::DBusError),
    FindingError(mpris::FindingError),
    Image(image::error::ImageError),
    Io(std::io::Error),
}

const DEFAULT_WALLPAPER: &str = "/usr/share/backgrounds/Alma-mountains-dark.xml";
const GENERATED_WALLPAPER: &str = "/tmp/cover-art.png";
const WIDTH: u32 = 2560;
const HEIGHT: u32 = 1440;
const BLUR: u32 = 32;

const MPRIS_SIGNAL_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const MPRIS_SIGNAL_MEMBER: &str = "PropertiesChanged";
const MPRIS_SIGNAL_OBJECT: &str = "/org/mpris/MediaPlayer2";

fn main() {
    // First open up a connection to the session bus.
    let conn = Connection::new_session().expect("D-Bus connection failed");

    // Second create a rule to match messages we want to receive; in this example we add no
    // further requirements, so all messages will match
    let mut rule = MatchRule::new();
    rule.interface = Some(dbus::strings::Interface::new(MPRIS_SIGNAL_INTERFACE).unwrap());
    rule.member = Some(dbus::strings::Member::new(MPRIS_SIGNAL_MEMBER).unwrap());
    rule.path = Some(dbus::strings::Path::new(MPRIS_SIGNAL_OBJECT).unwrap());

    // Try matching using new scheme
    let proxy = conn.with_proxy(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        Duration::from_millis(5000),
    );
    let result: Result<(), dbus::Error> = proxy.method_call(
        "org.freedesktop.DBus.Monitoring",
        "BecomeMonitor",
        (vec![rule.match_str()], 0u32),
    );

    if result.is_ok() {
        // Start matching using new scheme
        conn.start_receive(
            rule,
            Box::new(|msg, _| {
                handle_message(&msg);
                true
            }),
        );
    } else {
        // Start matching using old scheme
        rule.eavesdrop = true; // this lets us eavesdrop on *all* session messages, not just ours
        conn.add_match(rule, |_: (), _, msg| {
            handle_message(msg);
            true
        })
        .expect("add_match failed");
    }

    loop {
        conn.process(Duration::from_millis(1000)).unwrap();
    }
}

fn handle_message(msg: &Message) {
    // Track changes generate metadata events.
    // Unless we do this filtering, we also get volume events
    // which we do not want.
    let is_metadata = msg.get_items().iter().any(|m| match m {
        MessageItem::Dict(dict) => dict
            .iter()
            .any(|(key, _)| key == &MessageItem::Str("Metadata".into())),
        _ => false,
    });

    if !is_metadata {
        return;
    }

    let set_result = match gen_wallpaper() {
        Ok(_) => dconf_rs::set_string(
            "/org/gnome/desktop/background/picture-uri",
            &format!("file://{GENERATED_WALLPAPER}"),
        ),
        Err(e) => {
            eprintln!("{e:?}");
            dconf_rs::set_string(
                "/org/gnome/desktop/background/picture-uri",
                &format!("file://{DEFAULT_WALLPAPER}"),
            )
        }
    };
    match set_result {
        Ok(_) => println!("Wallpaper set"),
        Err(e) => eprintln!("Error: {e}"),
    }
}

fn gen_wallpaper() -> Result<(), Error> {
    let player = PlayerFinder::new()?.find_active()?;

    let metadata = player.get_metadata()?;

    let art_url = metadata
        .art_url()
        .ok_or_else(|| String::from("No art url found"))?;
    println!("Art url: {art_url}");

    let art_bytes = if art_url.starts_with("http") {
        let art_response = get(art_url)?;
        let art_download_status = art_response.status();
        if !art_download_status.is_success() {
            panic!(
                "Got {} {} while downloading art",
                art_download_status.as_u16(),
                art_download_status.as_str()
            );
        }

        art_response.bytes()?.to_vec()
    } else {
        std::fs::read(art_url.replace("file://", ""))?
    };

    let image_reader = image::io::Reader::new(Cursor::new(art_bytes)).with_guessed_format()?;
    let art = image_reader.decode()?.to_rgb8();

    let background = image_modifier::image_background(&art, [WIDTH, HEIGHT], Some(BLUR));
    let new_image = image_modifier::paste_images(
        &background,
        &art,
        [WIDTH, HEIGHT],
        [0, 0, WIDTH / 2, HEIGHT / 2],
    );

    new_image
        .save(GENERATED_WALLPAPER)
        .expect("Failed to save wallpaper");

    Ok(())
}
