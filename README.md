# TeleAPI

Simple HTTP RPC server that can:

* read a text file from filesystem and return it as a text
* read a binary file from filesystem and return it as an octet-stream
* write a file from POST data
* run shell script
* authenticate bearer token (API key)
* substitute URL prameters to shell script call or file name for file operations

## Example usage

* `teleapi.yaml`

```
---
listen: 127.0.0.1
listen_port: 8081
apikey: "abcd1234"
commands:
- endpoint: "/testread"
  read_file: "/tmp/apitest-{x}"
- endpoint: "/testwrite"
  write_file: "/tmp/apitest-{x}"
- endpoint: "/testshell"
  shell: 'echo Hello "x={x} y={y}"'
```

* run server: `rust run -- -c teleapi.yml` or `teleapi -c teleapi.yml`

* call `POST /testwrite` with `x=5`:

```
curl -vvv -H "Authorization: abcd1234" -X POST -d "1test2TEST3test4TeSt" "http://localhost:8081/testwrite?x=5"
```

* call `GET /testread` with `x=5`:

```
curl -vvv -H "Authorization: abcd1234" -X GET "http://localhost:8081/testread?x=5"
```

* call `GET /testshell` with `x=1234abcd y=xyz987`

```
curl -vvv -H "Authorization: abcd1234" -X GET "http://localhost:8081/testshell?x=1234abcd&y=xyz987"
```

Expected result:
```
{"retcode":0,"stdout":"Hello x=1234abcd y=xyz987\n","stderr":""}
```
