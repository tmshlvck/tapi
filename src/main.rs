/*
TeleAPI

Copyright (C) 2024 Tomas Hlavacek (tmshlvck@gmail.com)

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version.
This program is distributed in the hope that it will be useful, but WITHOUT
ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
You should have received a copy of the GNU General Public License along with
this program. If not, see <http://www.gnu.org/licenses/>.
*/

use rouille::{Request, Response, ResponseBody};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use nix::unistd::{Uid, Gid, chown};
use std::fs::File;
use clap::Parser;
use serde_derive::{Serialize, Deserialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Command {
    endpoint: String,
    shell: Option<String>,
    read_file: Option<String>,
    read_bin_file: Option<String>,
    write_file: Option<String>,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Config {
    listen: String,
    listen_port: u16,
    apikey: String,
    commands: Vec<Command>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct CommandResult {
    retcode: i32,
    stdout: String,
    stderr: String
}

impl CommandResult {
    fn from_output(out: std::process::Output) -> CommandResult {
        CommandResult {retcode: out.status.code().unwrap_or(0),
            stdout: String::from_utf8(out.stdout).unwrap_or(String::new()),
            stderr: String::from_utf8(out.stderr).unwrap_or(String::new()) }
    }
}

fn empty_with_status(code: u16) -> Response {
    Response {
        status_code: code,
        headers: vec![],
        data: ResponseBody::empty(),
        upgrade: None,
    }
}

fn read_bin_file(filename: &str) -> Response {
    let res = File::open(filename);
    match res {
        Ok(filecontent) => {
            println!("read_file {} success", filename);
            Response::from_file("application/octet-stream", filecontent)
        },
        Err(err) => {
            println!("read_file {} failed: {}", filename, err);
            Response::empty_404()
        }
    }
}

fn read_file(filename: &str) -> Response {
    let res = std::fs::read_to_string(filename);
    match res {
        Ok(cont) => {
            println!("read_file {} success", filename);
            Response::text(cont)
        },
        Err(err) => {
            println!("read_file {} failed: {}", filename, err);
            Response::empty_404()
        }
    }
}

fn write_file(cmd: &Command, request: &Request) -> Response{
    let cmdfn = cmd.write_file.clone().expect("File name to write is missing in command configration");
    let filename = expand_vars(&cmdfn, request);
    let mut rdata = match request.data() {
        Some(data) => data,
        None => {
            println!("write_file {} failed due to no data read from HTTP request.", filename);
            return empty_with_status(500);
        }
    };
    let mut sdata = String::new();
    match rdata.read_to_string(&mut sdata) {
        Ok(_) => {},
        Err(err) => {
            println!("write_file {} failed to extract data from HTTP request: {}", filename, err);
            return empty_with_status(500);
        }
    }

    match std::fs::write(&filename, sdata) {
        Ok(_) => {
            println!("write_file {} success", filename);         
        },
        Err(err) => {
            println!("write_file {} failed: {}", filename, err);
            return empty_with_status(500);
        }
    }

    match cmd.mode {
        Some(some_mode) => {
            let mut perms = match std::fs::metadata(&filename){
                Ok(md) => md.permissions(),
                Err(err) => {
                    println!("write_file {} failed to get metadata: {}", filename, err);
                    return empty_with_status(500);
                }
            };
            perms.set_mode(some_mode);
            match std::fs::set_permissions(&filename, perms) {
                Ok(_) => (),
                Err(err) => {
                    println!("write_file {} failed to set metadata: {}", filename, err);
                    return empty_with_status(500);
                }
            }
        },
        None => ()
    }

    match &cmd.user {
        Some(some_user) => {
            let uid = match users::get_user_by_name(some_user) {
                Some(u) => u.uid(),
                None => {
                    println!("write_file {} failed: user {} not found", filename, some_user);
                    return empty_with_status(500);
                }
            };
            match chown(filename.as_str(), Some(Uid::from_raw(uid)), None) {
                Ok(_) => (),
                Err(err) => {
                    println!("write_file {} failed to set uid: {}", filename, err);
                    return empty_with_status(500);
                }
            };
        },
        None => ()
    }

    match &cmd.group {
        Some(some_group) => {
            let gid = match users::get_group_by_name(some_group) {
                Some(g) => g.gid(),
                None => {
                    println!("write_file {} failed: group {} not found", filename, some_group);
                    return empty_with_status(500);
                }
            };
            match chown(filename.as_str(), None, Some(Gid::from_raw(gid))) {
                Ok(_) => (),
                Err(err) => {
                    println!("write_file {} failed to set gid: {}", filename, err);
                    return empty_with_status(500);
                }
            };
        },
        None => ()
    }

    empty_with_status(201)
}

fn shell(cmd: &str, request: &Request) -> Response {
    let mut sdata = String::new();
    match request.data() {
        Some(mut rdata) => rdata.read_to_string(&mut sdata).unwrap(),
        _ => 0
    };

    match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(output) => {
            let res = CommandResult::from_output(output);
            println!("command {} success: {:?}", cmd, res);
            Response::json(&res)
        },
        Err(err) => {
            println!("command {} failed: {}", cmd, err);
            empty_with_status(500)
        }
    }
}

fn expand_vars(input: &str, request: &Request) -> String {
    let maybestart = input.find('{');
    let maybeend = input.find('}');
    match (maybestart, maybeend) {
        (Some(start), Some(end)) => {
            //println!("start {} end {} param_name: {}", start, end, input[start..end+1].to_string());
            match request.get_param(&input[start+1..end]) {
                Some(p) => {
                    let out = input[..start].to_string() + &p + &input[end+1..];
                    return expand_vars(&out, request);
                }
                _ => ()
            };
        },
        _ => ()
    }

    String::from(input)
}

fn execute(cmd: &Command, request: &Request) -> Response {
    let method = request.method();
    println!("Request {} {} {}", method, request.remote_addr(), request.raw_url());
    match method {
        "POST" | "PUT" => {
            match &cmd.write_file {
                Some(_) => {
                    return write_file(cmd, request);
                },
                None => ()
            }

            match &cmd.shell {
                Some(shellcmd) => {
                    let expcmd = expand_vars(shellcmd, request);
                    return shell(&expcmd, request);
                },
                None => ()
            }

            println!("Unexpected method {} for endpoint {}", method, cmd.endpoint);
            return empty_with_status(500);
        },
        "GET" => {
            match &cmd.read_bin_file {
                Some(filename) => {
                    let expfilename = expand_vars(filename, request);
                    return read_bin_file(&expfilename);
                },
                None => ()
            }

            match &cmd.read_file {
                Some(filename) => {
                    let expfilename = expand_vars(filename, request);
                    return read_file(&expfilename);
                },
                None => ()
            }

            match &cmd.shell {
                Some(shellcmd) => {
                    let expcmd = expand_vars(shellcmd, request);
                    return shell(&expcmd, request);
                },
                None => ()
            }

            println!("Unexpected method {} for endpoint {}", method, cmd.endpoint);
            return empty_with_status(500);
        },

        "DELETE" => {
            println!("Unexpected DELETE method in endpoint {}", cmd.endpoint);
            empty_with_status(500)
        },
        
        _ => {
            println!("Unexpected method {} for endpoint {}", method, cmd.endpoint);
            empty_with_status(500)
        }
    }
}

fn check_auth(request: &Request, conf: &Config) -> bool {
    for h in request.headers() {
        let (k,v) = h;
        if k == "Authorization" && v == conf.apikey {
            return true;
        }
    }
    false
}

fn handle_http_request(request: &Request, conf: &Config) -> Response {
    if ! check_auth(request, conf) {
        println!("Authorizatin Failed: {} {} {}", request.method(), request.remote_addr(), request.url());
        return empty_with_status(401);
    }

    for c in &conf.commands {
        if c.endpoint == request.url() {
            return execute(c, request);
        }
    }

    println!("Command not found: {} {} from {}", request.method(), request.url(), request.remote_addr());
    Response::empty_404()
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = String::from("teleapi.yml"))]
    config: String,
}

fn main(){
    let args = Args::parse();

    let cf = std::fs::File::open(args.config).expect("Unable to open configuration file.");
    let conf: Config = serde_yaml::from_reader(cf).expect("Failed to parse configuration file.");

    println!("Starting server on {}:{}", conf.listen, conf.listen_port);

    rouille::start_server(format!("{}:{}", conf.listen, conf.listen_port),
        move |request| {handle_http_request(request, &conf)});
}
