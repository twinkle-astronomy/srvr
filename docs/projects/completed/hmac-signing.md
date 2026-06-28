# HMAC URL Signing and Time Abstraction

Secured the `/render/screen.bmp` endpoint with HMAC-SHA256 signatures so only the server can generate valid image URLs. Added a `Clock` trait (`RealClock` / `MockClock`) to make time-dependent signing logic fully testable without real wall-clock dependency. Signatures expire after 60 seconds with a 5-second leeway.
