use clap::Parser;
use imap::types::Fetch;
use imap::types::ZeroCopy;
use imap::Session;
use mail_parser::*;
use mail_send::SmtpClientBuilder;
use rustls::ClientConnection;
use rustls::StreamOwned;
use rustls_connector::RustlsConnector;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::{error::Error, net::TcpStream};
use std::{thread, time};

/// A mail retrieval agent that retrieves email using IMAP and forwards it to a different address using SMTP.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the config file with login information
    #[arg(short, long)]
    config: String,

    /// The interval in seconds to check for new emails. Use 0 for oneshot.
    #[arg(short, long, default_value = "600")]
    interval: u64,
}

#[derive(Deserialize, Serialize, Default, Debug, Hash)]
struct Config {
    imap_domain: String,
    imap_username: String,
    imap_password: String,
    smtp_domain: String,
    smtp_username: String,
    smtp_password: String,
    mailboxes: Vec<String>,
    forward_target: String,
}

fn open_session(
    config: &Config,
) -> Result<
    (
        Session<StreamOwned<ClientConnection, TcpStream>>,
        RustlsConnector,
    ),
    Box<dyn Error>,
> {
    // Setup Rustls TcpStream
    let stream = TcpStream::connect((config.imap_domain.as_ref(), 993))?;
    let tls = RustlsConnector::new_with_webpki_roots_certs();
    let tlsstream = tls.connect(&config.imap_domain, stream)?;

    // we pass in the domain twice to check that the server's TLS
    // certificate is valid for the domain we're connecting to.
    let client = imap::Client::new(tlsstream);

    // the client we have here is unauthenticated.
    // to do anything useful with the e-mails, we need to log in
    let imap_session = client
        .login(&config.imap_username, &config.imap_password)
        .map_err(|e| e.0)?;

    Ok((imap_session, tls))
}

fn fetch_unread_mail(
    session: &mut Session<StreamOwned<ClientConnection, TcpStream>>,
) -> Result<ZeroCopy<Vec<Fetch>>, Box<dyn Error>> {
    let unseen = session.uid_search("NOT SEEN")?;
    let unseen_str = unseen
        .iter()
        .map(|x| x.to_string())
        .collect::<Vec<String>>()
        .join(",");
    // session.uid_fetch(&unseen_str, "ALL")?;
    // TODO: issue a warning for large emails
    Ok(session.uid_fetch(&unseen_str, "BODY.PEEK[]")?)
}

fn parse_mail(mail: &Fetch) -> Result<Message, Box<dyn Error>> {
    let body = mail.body().ok_or("could not get body")?;
    let message = MessageParser::default()
        .parse(body)
        .ok_or("could not parse message")?;
    Ok(message)
}

fn build_forward_message<'a>(
    message: &'a Message,
    config: &'a Config,
) -> mail_send::smtp::message::Message<'a> {
    mail_send::smtp::message::Message::default()
        .to(config.forward_target.clone())
        .from(config.smtp_username.clone())
        .body(message.raw_message())
}

fn mark_as_seen(
    session: &mut Session<StreamOwned<ClientConnection, TcpStream>>,
    fetch: &Fetch,
) -> Result<ZeroCopy<Vec<Fetch>>, Box<dyn Error>> {
    Ok(session.uid_store(
        fetch.uid.ok_or("uid not found in fetch")?.to_string(),
        "+FLAGS (\\Seen)",
    )?)
}

#[tokio::main]
async fn send_mail(
    message: mail_send::smtp::message::Message,
    config: &Config,
) -> Result<(), Box<dyn Error>> {
    // Connect to the SMTP submissions port, upgrade to TLS and
    // authenticate using the provided credentials.
    let creds = mail_send::Credentials::Plain {
        username: &config.smtp_username.to_string(),
        secret: &config.smtp_password.to_string(),
    };
    let mut client = SmtpClientBuilder::new(&config.smtp_domain, 465)
        .implicit_tls(true)
        .credentials(creds)
        .connect()
        .await?;
    client.send(message).await?;
    Ok(())
}

fn run_full_cycle(config: &Config) -> Result<(), Box<dyn Error>> {
    let mut session = open_session(config)?;
    for mailbox in config.mailboxes.iter() {
        let info = session.0.select(mailbox)?;
        if info.unseen.unwrap_or(0) <= 0 {
            println!("No unseen mails in {mailbox}");
            continue;
        }
        let mbox = fetch_unread_mail(&mut session.0)?;
        println!("{} unseen mails in {mailbox}", mbox.len());
        for mail in mbox.iter() {
            let message = parse_mail(mail)?;
            let fwd = build_forward_message(&message, &config);
            send_mail(fwd, &config)?;
            mark_as_seen(&mut session.0, mail)?;
        }
    }
    session.0.logout()?;
    Ok(())
}

fn main() {
    let args = Args::parse();
    let configs = toml::from_str::<HashMap<String, Config>>(
        &fs::read_to_string(args.config).expect("Could not read config file"),
    )
    .expect("Could not parse config file");

    let interval = time::Duration::from_secs(args.interval);
    if args.interval <= 0 {
        println!("Running oneshot mode");
    } else {
        println!("Starting polling for new mails every {}s", args.interval);
    }
    loop {
        for (name, config) in configs.iter() {
            println!("Processing {name} with {}", config.imap_username);
            match run_full_cycle(config) {
                Ok(_) => println!("Successfully processed {name}"),
                Err(e) => println!("Error processing {name}: {e}"),
            }
        }
        if args.interval <= 0 {
            break;
        }
        thread::sleep(interval);
    }
}
