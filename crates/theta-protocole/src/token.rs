//! Distribution des `ConnectToken` netcode, par un court échange TCP.
//!
//! Seul le serveur connaît sa clé privée : le client lui demande un token avant
//! d'ouvrir la connexion UDP. Le service TCP écoute sur le même numéro de port
//! que le jeu en UDP.
//!
//! Requête : `PROTOCOL_ID` (u64 big-endian), puis l'adresse du serveur telle que
//! le client la voit, en texte terminé par `\n`. Le serveur ne peut pas la
//! deviner derrière un NAT, et le client s'y connectera d'après le token.
//!
//! Réponse : un octet de statut, suivi du token si le statut est [`STATUS_OK`].

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use lightyear::netcode::{CONNECT_TOKEN_BYTES, ConnectToken};

use crate::PROTOCOL_ID;

/// Le token suit.
pub const STATUS_OK: u8 = 0;
/// Le client ne parle pas la même version du protocole que le serveur.
pub const STATUS_PROTOCOL_MISMATCH: u8 = 1;

/// Délai maximal de chaque étape de l'échange, des deux côtés.
pub const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(3);

/// Longueur maximale de l'adresse envoyée par le client, `\n` compris.
const MAX_ADDR_LEN: u64 = 64;

/// Demande un token au serveur
pub fn request_token(server: SocketAddr) -> io::Result<ConnectToken> {
    let mut stream = TcpStream::connect_timeout(&server, EXCHANGE_TIMEOUT)?;
    stream.set_read_timeout(Some(EXCHANGE_TIMEOUT))?;
    stream.set_write_timeout(Some(EXCHANGE_TIMEOUT))?;

    let mut request = PROTOCOL_ID.to_be_bytes().to_vec();
    request.extend_from_slice(format!("{server}\n").as_bytes());
    stream.write_all(&request)?;

    let mut status = [0u8; 1];
    stream.read_exact(&mut status)?;
    match status[0] {
        STATUS_OK => {}
        STATUS_PROTOCOL_MISMATCH => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "le serveur utilise une autre version du protocole",
            ));
        }
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("statut de réponse inconnu : {other}"),
            ));
        }
    }

    let mut bytes = [0u8; CONNECT_TOKEN_BYTES];
    stream.read_exact(&mut bytes)?;
    ConnectToken::try_from_bytes(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Lit une requête de token : l'identifiant de protocole du client et
/// l'adresse par laquelle il joint le serveur.
pub fn read_request(stream: &mut TcpStream) -> io::Result<(u64, SocketAddr)> {
    let mut protocol_id = [0u8; 8];
    stream.read_exact(&mut protocol_id)?;

    let mut line = String::new();
    BufReader::new(stream.take(MAX_ADDR_LEN)).read_line(&mut line)?;
    let addr = line
        .trim_end()
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    Ok((u64::from_be_bytes(protocol_id), addr))
}

/// Répond à une requête par le token demandé.
pub fn write_token(stream: &mut TcpStream, token: ConnectToken) -> io::Result<()> {
    let bytes = token.try_into_bytes()?;
    stream.write_all(&[STATUS_OK])?;
    stream.write_all(&bytes)
}

/// Répond à une requête par un refus.
pub fn write_refusal(stream: &mut TcpStream, status: u8) -> io::Result<()> {
    stream.write_all(&[status])
}
