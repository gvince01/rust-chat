FROM rustlang/rust:nightly
MAINTAINER gvincent247@gmail.com

WORKDIR /var/www/microservice/
COPY .. .

RUN rustc --version
RUN cargo install

CMD ["microservice"]
