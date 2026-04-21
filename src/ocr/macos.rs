use anyhow::{Context, Result};
use objc2::runtime::AnyObject;
use objc2::{rc::Retained, AnyThread};
use objc2_foundation::{NSArray, NSData, NSDictionary};
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRequest,
    VNRequestTextRecognitionLevel,
};

use super::OcrEngine;

pub(crate) struct VisionOcrEngine;

impl OcrEngine for VisionOcrEngine {
    fn engine_name(&self) -> &'static str {
        "apple_vision"
    }

    fn recognition_level(&self) -> &'static str {
        "fast"
    }

    fn recognize_text(&self, image_bytes: &[u8]) -> Result<String> {
        objc2::rc::autoreleasepool(|_| recognize_text_with_vision(image_bytes))
    }
}

fn recognize_text_with_vision(image_bytes: &[u8]) -> Result<String> {
    let image_data = NSData::with_bytes(image_bytes);
    let options = NSDictionary::<VNImageOption, AnyObject>::new();
    let handler = VNImageRequestHandler::initWithData_options(
        VNImageRequestHandler::alloc(),
        &image_data,
        &options,
    );

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Fast);
    request.setUsesLanguageCorrection(true);
    request.setAutomaticallyDetectsLanguage(true);

    let request_for_array: Retained<VNRequest> = request.clone().into_super().into_super();
    let requests = NSArray::from_retained_slice(&[request_for_array]);
    handler
        .performRequests_error(&requests)
        .map_err(|error| anyhow::anyhow!("{}", error.localizedDescription()))?;

    let observations = request
        .results()
        .context("Vision returned no text-recognition results")?;
    let mut lines = Vec::new();
    for idx in 0..observations.len() {
        let observation = observations.objectAtIndex(idx);
        let candidates = observation.topCandidates(1);
        if candidates.is_empty() {
            continue;
        }
        let candidate = candidates.objectAtIndex(0);
        let text = candidate.string().to_string();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::VisionOcrEngine;
    use crate::ocr::OcrEngine;

    #[test]
    #[ignore = "requires the macOS Vision runtime"]
    fn vision_ocr_smoke_test_accepts_png_bytes() -> anyhow::Result<()> {
        let png = [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x08, 0x06, 0x00, 0x00,
            0x00, 0xc4, 0x0f, 0xbe, 0x8b, 0x00, 0x00, 0x00, 0x0f, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0xf8, 0x4f, 0x00, 0x30, 0x8c, 0x0c, 0x05, 0x00, 0x84, 0xb5, 0xff, 0x01,
            0x74, 0x42, 0xd9, 0x80, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42,
            0x60, 0x82,
        ];

        let _ = VisionOcrEngine.recognize_text(&png)?;
        Ok(())
    }
}
