import os
import shutil
import sys

import tensorflow as tf
from iree.compiler import tf as tfc
from PIL import Image
from transformers import AutoImageProcessor, TFResNetForImageClassification

# Download model and preprocessor from HuggingFace
processor = AutoImageProcessor.from_pretrained("microsoft/resnet-50")
model = TFResNetForImageClassification.from_pretrained(
    "microsoft/resnet-50", from_pt=True, use_safetensors=False
)

# Load image and process it
file_name = sys.argv[1]
image = Image.open(file_name).convert("RGB")
processed_image = processor(image, return_tensors="np")

# save raw bytes to file
new_file_name = os.path.splitext(file_name)[0] + ".bin"
try:
    os.remove(new_file_name)
except OSError:
    pass
with open(new_file_name, "wb") as f:
    f.write(processed_image["pixel_values"].tobytes())

# save model as saved_modelv2
try:
    shutil.rmtree("resnet50")
except OSError:
    pass


def model_exporter(model: tf.keras.Model):
    @tf.function(
        input_signature=[
            tf.TensorSpec([1, 3, 224, 224], tf.float32, name="pixel_values")
        ]
    )
    def serving_fn(pixel_values):
        return model(pixel_values=pixel_values).logits

    return serving_fn


model.save_pretrained(
    "resnet50", saved_model=True, signatures={"serving_default": model_exporter(model)}
)

# save id2label
try:
    os.remove("id2label.txt")
except OSError:
    pass
with open("id2label.txt", "w") as f:
    for i in range(len(model.config.id2label)):
        f.write(model.config.id2label[i] + "\n")

tfc.compile_saved_model(
    "./resnet50/saved_model/1/",
    output_file="resnet50.mlir",
    import_only=True,
    import_type=tfc.ImportType.SIGNATURE_DEF,
    exported_names=["serving_default"],
    saved_model_tags={"serve"},
)
