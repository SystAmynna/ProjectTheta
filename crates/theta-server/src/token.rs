use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
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

fn serve(listener: TcpListener, bind: SocketAddr, key: Key) {
    // Chaque token porte un `client_id` distinct : netcode refuse une seconde
    // connexion sous un identifiant déjà connecté.
    let mut next_client_id: u64 = 1;

    for stream in listener.incoming() {
        let result = stream.and_then(|mut stream| {
            let client_id = next_client_id;
            next_client_id += 1;
            answer(&mut stream, bind, key, client_id)
        });

        if let Err(error) = result {
            warn!("Requête de token rejetée : {error}");
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
}
