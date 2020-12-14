mod parser;

#[cfg(test)]
mod tests {
    use crate::parser::*;

    #[test]
    fn read_connect_req() {
        let mut reader = Reader::new(b"\x01\x00\x00\x00\x6c\x00\x00\x00\x00\x00\x00\x00");
        let req = Request::read(&mut reader).unwrap();

        assert_eq!(
            req,
            Request::Connect {
                client_auth_protocol_names: XimVec::new(vec![]),
                client_minor_protocol_version: 0,
                client_major_protocol_version: 0,
            }
        );
    }
}
