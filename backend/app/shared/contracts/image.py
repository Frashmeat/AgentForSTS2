from dataclasses import dataclass

from ..kernel.ids import ArtifactId


@dataclass(slots=True)
class ImageArtifact:
    artifact_id: ArtifactId
    provider: str
    uri: str
