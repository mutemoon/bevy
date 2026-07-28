use bevy_ecs::{
    reflect::AppTypeRegistry,
    world::{FromWorld, World},
};
use bevy_reflect::{TypePath, TypeRegistryArc};

#[cfg(feature = "serialize")]
use {
    crate::{serde::WorldDeserializer, DynamicWorld},
    bevy_asset::{io::Reader, AssetLoader, AssetPath, LoadContext, LoadFromPath, UntypedHandle},
    serde::de::DeserializeSeed,
    thiserror::Error,
};

/// Asset loader for a Bevy dynamic world (`.scn` / `.scn.ron`).
///
/// The loader handles assets serialized with [`DynamicWorld::serialize`].
#[derive(Debug, TypePath)]
pub struct WorldAssetLoader {
    #[cfg_attr(
        not(feature = "serialize"),
        expect(dead_code, reason = "only used with `serialize` feature")
    )]
    type_registry: TypeRegistryArc,
}

impl FromWorld for WorldAssetLoader {
    fn from_world(world: &mut World) -> Self {
        let type_registry = world.resource::<AppTypeRegistry>();
        WorldAssetLoader {
            type_registry: type_registry.0.clone(),
        }
    }
}

/// Possible errors that can be produced by [`WorldAssetLoader`]
#[cfg(feature = "serialize")]
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum WorldAssetLoaderError {
    /// An [IO Error](std::io::Error)
    #[error("Error while trying to read the world file: {0}")]
    Io(#[from] std::io::Error),
    /// A [RON Error](ron::error::SpannedError)
    #[error("Could not parse RON: {0}")]
    RonSpannedError(#[from] ron::error::SpannedError),
}

/// A `LoadFromPath` wrapper that catches panics and returns a default handle instead.
///
/// This allows deserialization to continue gracefully when asset types haven't been initialized,
/// rather than crashing with a panic.
#[cfg(feature = "serialize")]
struct SafeLoader<'a> {
    inner: &'a mut dyn LoadFromPath,
}

#[cfg(feature = "serialize")]
impl LoadFromPath for SafeLoader<'_> {
    fn load_from_path_erased(
        &mut self,
        type_id: core::any::TypeId,
        path: AssetPath<'static>,
    ) -> UntypedHandle {
        // Try to load, catching any panics (e.g., from HandleDeserializeProcessor
        // when an asset type hasn't been initialized)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.inner.load_from_path_erased(type_id, path)
        }));

        match result {
            Ok(handle) => handle,
            Err(_) => {
                // Return a default UUID handle if loading fails
                // This allows deserialization to continue with a placeholder handle
                UntypedHandle::default_for_type(type_id)
            }
        }
    }
}

#[cfg(feature = "serialize")]
impl AssetLoader for WorldAssetLoader {
    type Asset = DynamicWorld;
    type Settings = ();
    type Error = WorldAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let mut deserializer = ron::de::Deserializer::from_bytes(&bytes)?;
        let scene_deserializer = WorldDeserializer {
            type_registry: &self.type_registry.read(),
            load_from_path: &mut SafeLoader {
                inner: load_context,
            },
        };
        Ok(scene_deserializer
            .deserialize(&mut deserializer)
            .map_err(|e| deserializer.span_error(e))?)
    }

    fn extensions(&self) -> &[&str] {
        &["scn", "scn.ron"]
    }
}
