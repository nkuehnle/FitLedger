# OCR models

`text-detection.onnx` and `text-recognition.onnx` are the pre-trained
[Ocrs](https://github.com/robertknight/ocrs) models. They are not bundled with the app (they'd add
12 MB per CPU type): the app downloads them from the source below on the first sheet read and checks
these hashes (`ensure_models`). The copies here are for tests and the `dump` example.

Ocrs models (https://github.com/robertknight/ocrs-models), trained on the HierText dataset by
Google Research. Licensed under CC BY-SA 4.0 (https://creativecommons.org/licenses/by-sa/4.0/).

Source: `https://ocrs-models.s3-accelerate.amazonaws.com/<name>.onnx`

```
a917b23dbd9524b465df7e922641b3ff2981623df4ded5a0234004ef2fee7cfe  text-detection.onnx
86c145c2edb96c8caed5b1ebb8f44d706408922211451c309c625157dd6061c5  text-recognition.onnx
```
