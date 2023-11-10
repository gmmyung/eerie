import tensorflow as tf
from transformers import AutoImageProcessor, TFResNetForImageClassification
import sys
from PIL import Image
import os
from iree.compiler import tf as tfc
import inspect

# Download model and preprocessor from HuggingFace
processor = AutoImageProcessor.from_pretrained("microsoft/resnet-50")
model = TFResNetForImageClassification.from_pretrained("microsoft/resnet-50")

# Load image and process it
file_name = sys.argv[1]
image = Image.open(file_name).convert("RGB")
processed_image = processor(image, return_tensors="np")

# save raw bytes to file
new_file_name = file_name.split('.')[0] + '.bin'
try:
    os.remove(new_file_name)
except OSError:
    pass
with open(new_file_name, "wb") as f:
    f.write(processed_image['pixel_values'].tobytes())

# save model as saved_modelv2
try:
    os.rmdir("resnet50")
except OSError:
    pass

# fix model input shape to 1, 3, 224, 224
def model_exporter(model: tf.keras.Model):
    m_call = tf.function(model.call).get_concrete_function(
        tf.TensorSpec(
            shape=[None, 3, 224, 224], dtype=tf.float32, name='pixel_values'
        )
    )
    
    @tf.function(input_signature=[tf.TensorSpec([1, 3, 224, 224], tf.float32)])
    def serving_fn(input):
        return model(**processed_image).logits

    return serving_fn

model.save_pretrained("resnet50", saved_model=True, signatures={'serving_default': model_exporter(model)})

# save id2label
try:
    os.remove("id2label.txt")
except OSError:
    pass
with open("id2label.txt", "w") as f:
    for i in range(len(model.config.id2label)):
        f.write(model.config.id2label[i] + '\n')

# iree-tools-tf --tf-import-type=savedmodel_v1 ./resnet50/saved_model/1/ -o resnet50.mlir
import subprocess
subprocess.run(["iree-import-tf", 
    "--tf-import-type=savedmodel_v1", 
    "--tf-savedmodel-exported-names=serving_default", 
    "./resnet50/saved_model/1/", 
    "-o", 
    "resnet50.mlir"])
