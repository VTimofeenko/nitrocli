// pinentry.rs

// *************************************************************************
// * Copyright (C) 2017-2019 Daniel Mueller (deso@posteo.net)              *
// *                                                                       *
// * This program is free software: you can redistribute it and/or modify  *
// * it under the terms of the GNU General Public License as published by  *
// * the Free Software Foundation, either version 3 of the License, or     *
// * (at your option) any later version.                                   *
// *                                                                       *
// * This program is distributed in the hope that it will be useful,       *
// * but WITHOUT ANY WARRANTY; without even the implied warranty of        *
// * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
// * GNU General Public License for more details.                          *
// *                                                                       *
// * You should have received a copy of the GNU General Public License     *
// * along with this program.  If not, see <http://www.gnu.org/licenses/>. *
// *************************************************************************

use std::borrow;
use std::fmt;
use std::process;
use std::str;

use crate::error::Error;

type CowStr = borrow::Cow<'static, str>;

/// PIN type requested from pinentry.
///
/// The available PIN types correspond to the PIN types used by the Nitrokey devices:  user and
/// admin.
Enum! {PinType, [
  Admin => "admin",
  User => "user"
]}

/// A trait representing a secret to be entered by the user.
pub trait SecretEntry: fmt::Debug {
  /// The cache ID to use for this secret.
  fn cache_id(&self) -> CowStr;
  /// The prompt to display when asking for the secret.
  fn prompt(&self) -> CowStr;
  /// The description to display when asking for the secret.
  fn description(&self, mode: Mode) -> CowStr;
}

#[derive(Debug)]
pub struct PinEntry {
  pin_type: PinType,
  model: nitrokey::Model,
  serial: String,
}

impl PinEntry {
  pub fn from<D>(pin_type: PinType, device: &D) -> crate::Result<Self>
  where
    D: nitrokey::Device,
  {
    let model = device.get_model();
    let serial = device.get_serial_number()?;
    Ok(Self {
      pin_type,
      model,
      serial,
    })
  }

  pub fn pin_type(&self) -> PinType {
    self.pin_type
  }
}

impl SecretEntry for PinEntry {
  fn cache_id(&self) -> CowStr {
    let model = self.model.to_string().to_lowercase();
    let suffix = format!("{}:{}", model, self.serial);

    match self.pin_type {
      PinType::Admin => format!("nitrocli:admin:{}", suffix),
      PinType::User => format!("nitrocli:user:{}", suffix),
    }
    .into()
  }

  fn prompt(&self) -> CowStr {
    match self.pin_type {
      PinType::Admin => "Admin PIN",
      PinType::User => "User PIN",
    }
    .into()
  }

  fn description(&self, mode: Mode) -> CowStr {
    format!(
      "{} for\rNitrokey {} {}",
      match self.pin_type {
        PinType::Admin => match mode {
          Mode::Choose => "Please enter a new admin PIN",
          Mode::Confirm => "Please confirm the new admin PIN",
          Mode::Query => "Please enter the admin PIN",
        },
        PinType::User => match mode {
          Mode::Choose => "Please enter a new user PIN",
          Mode::Confirm => "Please confirm the new user PIN",
          Mode::Query => "Please enter the user PIN",
        },
      },
      self.model,
      self.serial,
    )
    .into()
  }
}

/// Secret entry mode for pinentry.
///
/// This enum describes the context of the pinentry query, for example
/// prompting for the current secret or requesting a new one. The mode
/// may affect the pinentry description and whether a quality bar is
/// shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
  /// Let the user choose a new secret.
  Choose,
  /// Let the user confirm the previously chosen secret.
  Confirm,
  /// Query an existing secret.
  Query,
}

impl Mode {
  fn show_quality_bar(self) -> bool {
    self == Mode::Choose
  }
}

fn parse_pinentry_pin<R>(response: R) -> crate::Result<String>
where
  R: AsRef<str>,
{
  let string = response.as_ref();
  let lines: Vec<&str> = string.lines().collect();

  // We expect the response to be of the form:
  // > D passphrase
  // > OK
  // or potentially:
  // > ERR 83886179 Operation cancelled <Pinentry>
  if lines.len() == 2 && lines[1] == "OK" && lines[0].starts_with("D ") {
    // We got the only valid answer we accept.
    let (_, pass) = lines[0].split_at(2);
    return Ok(pass.to_string());
  }

  // Check if we are dealing with a special "ERR " line and report that
  // specially.
  if !lines.is_empty() && lines[0].starts_with("ERR ") {
    let (_, error) = lines[0].split_at(4);
    return Err(Error::from(error));
  }
  Err(Error::Error(format!("Unexpected response: {}", string)))
}

/// Inquire a secret from the user.
///
/// This function inquires a secret from the user or returns a cached
/// entry, if available. If an error message is set, it is displayed in
/// the entry dialog. The mode describes the context of the pinentry
/// dialog. It is used to choose an appropriate description and to
/// decide whether a quality bar is shown in the dialog.
pub fn inquire<E>(entry: &E, mode: Mode, error_msg: Option<&str>) -> crate::Result<String>
where
  E: SecretEntry,
{
  let cache_id = entry.cache_id().into();
  let error_msg = error_msg
    .map(|msg| msg.replace(" ", "+"))
    .unwrap_or_else(|| String::from("+"));
  let prompt = entry.prompt().replace(" ", "+");
  let description = entry.description(mode).replace(" ", "+");

  let args = vec![cache_id, error_msg, prompt, description].join(" ");
  let mut command = "GET_PASSPHRASE --data ".to_string();
  if mode.show_quality_bar() {
    command += "--qualitybar ";
  }
  command += &args;
  // An error reported for the GET_PASSPHRASE command does not actually
  // cause gpg-connect-agent to exit with a non-zero error code, we have
  // to evaluate the output to determine success/failure.
  let output = process::Command::new("gpg-connect-agent")
    .arg(command)
    .arg("/bye")
    .output()?;
  parse_pinentry_pin(str::from_utf8(&output.stdout)?)
}

fn check(pin_type: PinType, secret: &str) -> crate::Result<()> {
  let minimum_length = match pin_type {
    PinType::Admin => 8,
    PinType::User => 6,
  };
  if secret.len() < minimum_length {
    Err(Error::Error(format!(
      "The secret must be at least {} characters long",
      minimum_length
    )))
  } else {
    Ok(())
  }
}

pub fn choose(entry: &PinEntry) -> crate::Result<String> {
  clear(entry)?;
  let chosen = inquire(entry, Mode::Choose, None)?;
  clear(entry)?;
  check(entry.pin_type(), &chosen)?;

  let confirmed = inquire(entry, Mode::Confirm, None)?;
  clear(entry)?;

  if chosen != confirmed {
    Err(Error::from("Entered secrets do not match"))
  } else {
    Ok(chosen)
  }
}

fn parse_pinentry_response<R>(response: R) -> crate::Result<()>
where
  R: AsRef<str>,
{
  let string = response.as_ref();
  let lines = string.lines().collect::<Vec<_>>();

  if lines.len() == 1 && lines[0] == "OK" {
    // We got the only valid answer we accept.
    return Ok(());
  }
  Err(Error::Error(format!("Unexpected response: {}", string)))
}

/// Clear the cached secret represented by the given entry.
pub fn clear<E>(entry: &E) -> crate::Result<()>
where
  E: SecretEntry,
{
  let command = format!("CLEAR_PASSPHRASE {}", entry.cache_id());
  let output = process::Command::new("gpg-connect-agent")
    .arg(command)
    .arg("/bye")
    .output()?;

  parse_pinentry_response(str::from_utf8(&output.stdout)?)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_pinentry_pin_good() {
    let response = "D passphrase\nOK\n";
    let expected = "passphrase";

    assert_eq!(parse_pinentry_pin(response).unwrap(), expected)
  }

  #[test]
  fn parse_pinentry_pin_error() {
    let error = "83886179 Operation cancelled";
    let response = "ERR ".to_string() + error + "\n";
    let expected = error;

    let error = parse_pinentry_pin(response.to_string());

    if let Error::Error(ref e) = error.err().unwrap() {
      assert_eq!(e, &expected);
    } else {
      panic!("Unexpected result");
    }
  }

  #[test]
  fn parse_pinentry_pin_unexpected() {
    let response = "foobar\n";
    let expected = format!("Unexpected response: {}", response);
    let error = parse_pinentry_pin(response);

    if let Error::Error(ref e) = error.err().unwrap() {
      assert_eq!(e, &expected);
    } else {
      panic!("Unexpected result");
    }
  }

  #[test]
  fn parse_pinentry_response_ok() {
    assert!(parse_pinentry_response("OK\n").is_ok())
  }

  #[test]
  fn parse_pinentry_response_ok_no_newline() {
    assert!(parse_pinentry_response("OK").is_ok())
  }

  #[test]
  fn parse_pinentry_response_unexpected() {
    let response = "ERR 42";
    let expected = format!("Unexpected response: {}", response);
    let error = parse_pinentry_response(response);

    if let Error::Error(ref e) = error.err().unwrap() {
      assert_eq!(e, &expected);
    } else {
      panic!("Unexpected result");
    }
  }
}
