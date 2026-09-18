use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use bevy::prelude::*;
use lightyear::netcode::{ConnectToken, Key};
use theta_protocole::PROTOCOL_ID;
use theta_protocole::token::{
    EXCHANGE_TIMEOUT, STATUS_PROTOCOL_MISMATCH, read_request, write_refusal, write_token,
};

/// Délai après lequel le serveur coupe un client silencieux, en secondes.
const CLIENT_TIMEOUT_SECS: i32 = 3;
/// Durée de validité d'un token émis, en secondes : le client s'en sert aussitôt.
const TOKEN_EXPIRE_SECS: i32 = 30;
/// Nombre maximal d'échanges traités en même temps. Au-delà, les nouvelles
/// connexions sont fermées aussitôt : le client réessaiera.
const MAX_CONCURRENT_EXCHANGES: usize = 32;

/// Lance, sur son propre thread, le service qui distribue les tokens de
/// connexion. La clé privée n'existe que là et dans le `NetcodeServer`.
pub fn spawn_token_service(bind: SocketAddr, key: Key) -> io::Result<()> {
    let listener = TcpListener::bind(bind)?;

    thread::Builder::new()
        .name("theta-tokens".into())
        .spawn(move || serve(listener, bind, key))?;

    info!("Service de tokens en écoute sur {bind} (TCP)");
    Ok(())
}

/// Accepte les connexions et traite chacune sur son propre thread : un client
/// lent ou muet ne retient que le sien, jusqu'à [`EXCHANGE_TIMEOUT`], sans
/// bloquer les suivants.
fn serve(listener: TcpListener, bind: SocketAddr, key: Key) {
    // Chaque token porte un `client_id` distinct : netcode refuse une seconde
    // connexion sous un identifiant déjà connecté.
    let mut next_client_id: u64 = 1;
    let in_flight = Arc::new(AtomicUsize::new(0));

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                warn!("Connexion au service de tokens échouée : {error}");
                continue;
            }
        };

        if in_flight.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT_EXCHANGES {
            in_flight.fetch_sub(1, Ordering::AcqRel);
            warn!("Service de tokens saturé : connexion fermée");
            continue;
        }

        let client_id = next_client_id;
        next_client_id += 1;
        let exchange = Arc::clone(&in_flight);
        let spawned = thread::Builder::new()
            .name("theta-token".into())
            .spawn(move || {
                if let Err(error) = answer(&mut stream, bind, key, client_id) {
                    warn!("Requête de token rejetée : {error}");
                }
                exchange.fetch_sub(1, Ordering::AcqRel);
            });
        if let Err(error) = spawned {
            warn!("Impossible de traiter une requête de token : {error}");
            in_flight.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn answer(stream: &mut TcpStream, bind: SocketAddr, key: Key, client_id: u64) -> io::Result<()> {
    stream.set_read_timeout(Some(EXCHANGE_TIMEOUT))?;
    stream.set_write_timeout(Some(EXCHANGE_TIMEOUT))?;

    let (protocol_id, public_addr) = read_request(stream)?;
    if protocol_id != PROTOCOL_ID {
        write_refusal(stream, STATUS_PROTOCOL_MISMATCH)?;
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("protocole {protocol_id:#x} au lieu de {PROTOCOL_ID:#x}"),
        ));
    }

    // L'adresse publique est celle où le client enverra ses paquets ; l'adresse
    // interne, chiffrée dans le token, est celle que le serveur vérifie contre
    // son `LocalAddr`.
    let token = ConnectToken::build(public_addr, PROTOCOL_ID, client_id, key)
        .internal_addresses(bind)
        .and_then(|builder| {
            builder
                .timeout_seconds(CLIENT_TIMEOUT_SECS)
                .expire_seconds(TOKEN_EXPIRE_SECS)
                .generate()
        })
        .map_err(io::Error::other)?;

    write_token(stream, token)?;
    info!("Token émis pour le client {client_id} ({public_addr})");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use lightyear::netcode::generate_key;
    use theta_protocole::token::request_token;

    use super::*;

    /// Démarre le service sur un port éphémère et renvoie son adresse.
    fn start() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || serve(listener, addr, generate_key()));
        addr
    }

    #[test]
    fn delivers_a_token() {
        let addr = start();
        assert!(request_token(addr).is_ok());
        assert!(request_token(addr).is_ok());
    }

    #[test]
    fn refuses_another_protocol() {
        let addr = start();
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(&(PROTOCOL_ID + 1).to_be_bytes()).unwrap();
        stream.write_all(format!("{addr}\n").as_bytes()).unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        assert_eq!(response, [STATUS_PROTOCOL_MISMATCH]);
    }

    #[test]
    fn a_silent_client_does_not_block_the_others() {
        let addr = start();
        // Connecté mais muet : il occupe son échange jusqu'au délai maximal.
        let _silent = TcpStream::connect(addr).unwrap();

        let started = std::time::Instant::now();
        assert!(request_token(addr).is_ok());
        assert!(
            started.elapsed() < EXCHANGE_TIMEOUT,
            "le token a attendu le client muet : {:?}",
            started.elapsed()
        );
    }
}
