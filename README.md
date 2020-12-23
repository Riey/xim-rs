# xim-rs

XIM protocol handler in Rust

## project structure

### xim

Binding with X client libraries

### xim-parser

Read/Write xim message generated from xim-gen

### xim-gen

xim protocol parser generator

## features

- [x] Parse messages
- [x] Basic protocol
- [ ] Extension protocol
- [x] AttributeBuilder

## binding for X client

#### xlib
- [x] client
- [ ] server

#### x11rb
- [x] client
- [ ] server

## limitations

* Only native endian is supported
* Only support utf-8 mode of CTEXT
* Auth, StrConvertion doesn't supported since they are not used in real world
