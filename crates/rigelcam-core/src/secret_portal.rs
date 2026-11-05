//! Passwords in the Flatpak build. The sandbox has no access to the keyring of the session, so they
//! go to a file of this app, encrypted with a key that the Secret portal hands out.

use std::collections::HashMap;

use futures_lite::future::block_on;
use oo7::Keyring;

use crate::recorders::Error;

fn failed(error: oo7::Error) -> Error {
    Error::Keyring(error.to_string())
}

fn attributes<'a>(service: &'a str, account: &'a str) -> HashMap<&'a str, &'a str> {
    HashMap::from([("service", service), ("username", account)])
}

pub fn set(service: &str, account: &str, password: &str) -> Result<(), Error> {
    block_on(async {
        let keyring = Keyring::new().await.map_err(failed)?;
        let label = format!("{service}: {account}");
        keyring
            .create_item(&label, &attributes(service, account), password, true)
            .await
            .map_err(failed)
    })
}

pub fn get(service: &str, account: &str) -> Result<String, Error> {
    block_on(async {
        let keyring = Keyring::new().await.map_err(failed)?;
        let items = keyring
            .search_items(&attributes(service, account))
            .await
            .map_err(failed)?;
        let item = items.first().ok_or(Error::NoPassword)?;
        let secret = item.secret().await.map_err(failed)?;
        String::from_utf8(secret.as_bytes().to_vec())
            .map_err(|_| Error::Keyring(format!("the password of {account} is not text")))
    })
}

pub fn delete(service: &str, account: &str) -> Result<(), Error> {
    block_on(async {
        let keyring = Keyring::new().await.map_err(failed)?;
        keyring
            .delete(&attributes(service, account))
            .await
            .map_err(failed)
    })
}
