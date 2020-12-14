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

    #[test]
    fn read_query() {
        let req: Request = read(&[
            40, 0, 5, 0, 0, 0, 13, 0, 12, 88, 73, 77, 95, 69, 88, 84, 95, 77, 79, 86, 69, 0, 0, 0,
        ])
        .unwrap();
        assert_eq!(
            req,
            Request::QueryExtension {
                input_method_id: 0,
                extensions: vec![XimString(b"XIM_EXT_MOVE"),],
            }
        );
    }

    #[test]
    fn write_connect_reply() {
        let reply = Request::ConnectReply {
            server_minor_protocol_version: 0,
            server_major_protocol_version: 1,
        };
        let mut out = Vec::new();
        write(&reply, &mut out);

        assert_eq!(out, b"\x02\x00\x01\x00\x01\x00\x00\x00");
    }
}
