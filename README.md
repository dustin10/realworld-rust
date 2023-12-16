
# RealWorld

An application written in Rust that adheres to the [RealWorld](https://github.com/gothinkster/realworld) specification. For
more information on the specification head over to the [RealWorld](https://github.com/gothinkster/realworld) repo.

The application goes a little further that what is defined in the specification aand adds the following functionality as well.

* Simple implementation of the [Transactional Outbox](https://microservices.io/patterns/data/transactional-outbox.html) pattern for publishing events
* A simple Kafka event consumer

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

# initialize kafka topics
> ./init-kafka.sh

# run the application
> cargo run
```

## Running API Tests

A script to run tests using a Postman collection is provided in the `api-tests` folder. Assuming the application is
already running, the following commands can be executed to run the API tests.

```sh
# change to the dir with the script and supporting files
> cd api-tests

# run the tests specifying the host and port of your application
> APIURL=http://localhost:7100/api ./run-api-tests.sh
```

> Note that the script requires [npx](https://github.com/npm/npx) to be installed.
