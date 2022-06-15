//! Derive macros for kanin and other meta-helpers.

pub use kanin_derive::FromError;

/// Extension trait to prost_build to make it easier to list all the types that should derive FromError.
pub trait ProstDeriveExt<P> {
    /// Derives the FromError trait in the appropriate way for all the given types.
    fn derive_from_error(&mut self, paths: &[P]) -> &mut Self
    where
        P: AsRef<str>;
}

impl<P> ProstDeriveExt<P> for prost_build::Config {
    fn derive_from_error(&mut self, paths: &[P]) -> &mut Self
    where
        P: AsRef<str>,
    {
        for path in paths {
            self.type_attribute(path, "#[derive(::kanin::derive::FromError)]");
        }

        self
    }
}
