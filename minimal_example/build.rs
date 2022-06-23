fn main() {
    prost_build::Config::new()
        .out_dir("src/protobuf/generated")
        // We need to derive FromError on any type that we want to use
        // as a return type from a handler. So we just list them here.
        // These are paths into the .proto file, not the generated Rust code.
        .type_attribute("InvalidRequest", "#[derive(kanin::derive::FromError)]")
        .type_attribute("InternalError", "#[derive(kanin::derive::FromError)]")
        .type_attribute("EchoResponse", "#[derive(kanin::derive::FromError)]")
        .type_attribute(
            "EchoResponse.response",
            "#[derive(kanin::derive::FromError)]",
        )
        .compile_protos(&["src/protobuf/echo.proto"], &["src"])
        .expect("Failed to compile .proto files")
}
