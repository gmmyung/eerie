# Example
This example runs a tensorflow resnet50 model using Eerie. 

## Prerequisites
A Python environment installed is needed to translate the saved model into StableHLO format.

```sh
pip3 install tensorflow transformers
```

```sh
cd examples/
python3 load_mlir.py <path to image>
```
This python script will preprocess the given image into a binary file, and also export resnet50 model from huggingface into a stablehlo MLIR bytecode.

```sh
cd ..
cargo run --example resnet
```
