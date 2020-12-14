mod parser;

#[cfg(test)]
mod tests {
    use crate::parser::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn read_connect_req() {
        let req: Request = read(b"\x01\x00\x00\x00\x6c\x00\x00\x00\x00\x00\x00\x00").unwrap();

        assert_eq!(
            req,
            Request::Connect {
                endian: Endian::Little,
                client_auth_protocol_names: vec![],
                client_minor_protocol_version: 0,
                client_major_protocol_version: 0,
            }
        );
    }

    #[test]
    fn read_open() {
        let req: Request = read(&[
            30, 0, 2, 0, 5, 101, 110, 95, 85, 83, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])
        .unwrap();
        assert_eq!(
            req,
            Request::Open {
                name: XimString(b"en_US"),
            }
        );
    }
}
