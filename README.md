
# RealWorld

An application written in Rust that adheres to the [RealWorld](https://github.com/gothinkster/realworld) specification. For
more information on the specification head over to the [RealWorld](https://github.com/gothinkster/realworld) repo. This
application goes a little further than what is defined in the RealWorld specification. The following additional functionality
is also implemented.

* Transactional Outbox pattern to support guaranteed event delivery to Kafka
* Health check API endpoint

## Stack

* Language - [Rust](https://www.rust-lang.org/)
* HTTP - [axum](https://docs.rs/axum/latest/axum/)
* Database - [PostgreSQL](https://www.postgresql.org/)
* Events - [Kafka](https://kafka.apache.org/)

A `docker-compose.yml` file is provided so that all dependencies of the application can be run locally which requires
[Docker](https://www.docker.com/) to be installed.

## Getting Started

``` sh
# set the password to use for the db
> export RW_DATABASE_PASSWORD=<password>

# set the signing key used to sign and validate auth tokens
> export RW_SIGNING_KEY=<signing-key>

# start the infrastructure required to run the application
> docker-compose up -d

# run the application
> cargo run
```
