
# RealWorld

An application written in Rust that adheres to the [RealWorld](https://github.com/gothinkster/realworld) specification. For
more information on the specification head over to the [RealWorld](https://github.com/gothinkster/realworld) repo. This
application goes a little further than what is defined in the RealWorld specification. The following additional functionality
is also implemented.

* Implements a Transaction Outbox to support guaranteed event publishing

## Stack

* Language - [Rust](https://www.rust-lang.org/)
* HTTP - [axum](https://docs.rs/axum/latest/axum/)
* Database - [PostgreSQL](https://www.postgresql.org/)
* Events - [Kafka](https://kafka.apache.org/)

A `docker-compose.yml` file is provided so that all dependencies of the application can be run locally which requires
[Docker](https://www.docker.com/) to be installed.

## Getting Started

``` sh
> export RW_DATABASE_PASSWORD=<password>
> docker-compose up -d
> cargo run
```
