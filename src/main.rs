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

fn write_file(filename: &str, request: &Request) -> Response {
    let mut rdata = request.data().unwrap();
    let mut sdata = String::new();
    rdata.read_to_string(&mut sdata).unwrap();
    let res = std::fs::write(filename, sdata);
    match res {
        Ok(_) => {
            println!("write_file {} success", filename);
            empty_with_status(201)
        },
        Err(err) => {
            println!("write_file {} failed: {}", filename, err);
            empty_with_status(500)
        }
    }
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

fn empty_with_status(code: u16) -> Response {
    Response {
        status_code: code,
        headers: vec![],
        data: ResponseBody::empty(),
        upgrade: None,
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
                _ => {}
            };
        },
        _ => {}
    }

    String::from(input)
}

fn execute(cmd: &Command, request: &Request) -> Response {
    let method = request.method();
    println!("Request {} {} {}", method, request.remote_addr(), request.raw_url());
    match method {
        "POST" | "PUT" => {
            match &cmd.write_file {
                Some(filename) => {
                    let expfn = expand_vars(filename, request);
                    return write_file(&expfn, request);
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
                    let expfn = expand_vars(filename, request);
                    return read_bin_file(&expfn);
                },
                None => ()
            }

            match &cmd.read_file {
                Some(filename) => {
                    let expfn = expand_vars(filename, request);
                    return read_file(&expfn);
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

    let cf = std::fs::File::open(args.config).unwrap();
    let conf: Config = serde_yaml::from_reader(cf).unwrap();

    println!("Starting server on {}:{}", conf.listen, conf.listen_port);

    rouille::start_server(format!("{}:{}", conf.listen, conf.listen_port),
        move |request| {handle_http_request(request, &conf)});
}
