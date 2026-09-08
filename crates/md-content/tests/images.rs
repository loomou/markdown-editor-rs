use gpui::http_client::{AsyncBody, FakeHttpClient, Response};
use md_content::images::{self, ImageCache, Resolved, SourceError};

fn failure() -> md_content::Error {
    md_content::Error::Image("diagnostic fixture failure".to_string().into())
}

fn status_client(
    status: u16,
    retry_after: Option<&str>,
) -> std::sync::Arc<gpui::http_client::HttpClientWithUrl> {
    let retry_after = retry_after.map(str::to_string);
    FakeHttpClient::create(move |_| {
        let retry_after = retry_after.clone();
        async move {
            let mut builder = Response::builder().status(status);
            if let Some(value) = retry_after {
                builder = builder.header("Retry-After", value);
            }
            Ok(builder.body(AsyncBody::default()).unwrap())
        }
    })
}

fn fixture_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "md-test-img-pct-{}-{tag}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_png(path: &std::path::Path) {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    img.save(path).unwrap();
}

fn load_local(cx: &mut gpui::TestAppContext, dest: &str, source: &std::path::Path) -> bool {
    let resolved = images::resolve(dest, Some(source)).expect("resolve");
    let future = cx.update(|app| images::load_source(resolved, false, app));
    futures::executor::block_on(future).is_ok()
}

#[test]
fn unc_targets_are_not_classified_local() {
    for dest in [
        r"\\host\share\a.png",
        "//host/share/a.png",
        r"\\?\UNC\host\share\a.png",
        "file://host/share/a.png",
        "file://HOST/share/a.png",
    ] {
        let got = images::resolve(dest, None);
        assert!(
            matches!(got, Some(Resolved::Network(_))),
            "{dest:?} classified as {got:?}: network targets must take the network path"
        );
    }
}

#[test]
fn local_targets_still_classify_as_local() {
    for dest in [
        r"\\?\C:\a.png",
        r"\\.\COM1",
        "file:///C:/a.png",
        "file://localhost/C:/a.png",
        r"C:\images\a.png",
    ] {
        let got = images::resolve(dest, None);
        assert!(
            matches!(got, Some(Resolved::Local(_))),
            "{dest:?} must stay Local, got {got:?}"
        );
    }
}

#[test]
fn relative_join_cannot_smuggle_a_unc_target() {
    let src = std::path::PathBuf::from("L:").join("md").join("doc.md");
    for dest in [r"\\host\share\a.png", "//host/share/a.png"] {
        let got = images::resolve(dest, Some(&src));
        assert!(
            matches!(got, Some(Resolved::Network(_))),
            "{dest:?} via relative classified as {got:?}: must take the network path"
        );
    }
}

#[gpui::test]
fn network_targets_are_gated_by_the_remote_switch(cx: &mut gpui::TestAppContext) {
    for allow_remote in [false, true] {
        let resolved = images::resolve(r"\\host\share\a.png", None).expect("resolve");
        let future = cx.update(|app| images::load_source(resolved, allow_remote, app));
        let err = futures::executor::block_on(future)
            .expect_err("network targets must never reach load_local");
        assert!(
            matches!(err, SourceError::Fatal(_)),
            "the outcome must be final for allow_remote={allow_remote}: {err:?}"
        );
    }
}

#[test]
fn local_normalization_is_lexical_for_missing_paths() {
    let src = std::path::PathBuf::from("L:").join("md").join("doc.md");
    let got = images::resolve("definitely-missing.png", Some(&src)).expect("resolve");
    match got {
        Resolved::Local(p) => {
            let expected =
                std::path::absolute(src.parent().unwrap().join("definitely-missing.png"))
                    .expect("absolute");
            assert_eq!(
                p, expected,
                "normalization must be the lexical absolute path"
            );
        }
        other => panic!("expected local, got {other:?}"),
    }
}

#[gpui::test]
fn clear_must_reject_an_old_source_result(cx: &mut gpui::TestAppContext) {
    cx.update(|app| {
        let mut cache = ImageCache::new();
        let key = "https://example.invalid/image.png".to_string();

        let old = cache.begin_source(key.clone()).expect("begin");
        cache.clear(app);
        let new = cache.begin_source(key.clone()).expect("re-entry");

        let frame = image::Frame::new(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([0, 0, 255, 255]),
        ));
        let image = std::sync::Arc::new(gpui::RenderImage::new(vec![frame]));
        let accepted_old = cache.finish_source(old, Ok((image, 2, 2)), app);
        assert!(
            !accepted_old,
            "clear must void the old job's identity, not just the map entry"
        );

        let accepted_new = cache.finish_source(new, Err(SourceError::Fatal(failure())), app);
        assert!(accepted_new, "the new job's failure must be booked");
        assert!(
            cache.source_image(&key).is_none(),
            "after the old result was refused, the source must not be Ready"
        );
    });
}

#[gpui::test]
fn percent_encoded_local_images_resolve_and_load(cx: &mut gpui::TestAppContext) {
    let dir = fixture_dir("cjk");
    write_png(&dir.join("image with spaces.png"));
    write_png(&dir.join("中文名.png"));
    let source = dir.join("document.md");
    std::fs::write(&source, "").unwrap();

    assert!(
        load_local(cx, "image%20with%20spaces.png", &source),
        "the percent-encoding of a space must decode to the real file name"
    );
    assert!(
        load_local(cx, "%E4%B8%AD%E6%96%87%E5%90%8D.png", &source),
        "the percent-encoding of UTF-8 (CJK) must decode to the real file name"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn plain_local_images_still_load(cx: &mut gpui::TestAppContext) {
    let dir = fixture_dir("plain");
    write_png(&dir.join("image with spaces.png"));
    write_png(&dir.join("中文名.png"));
    let source = dir.join("document.md");
    std::fs::write(&source, "").unwrap();

    assert!(
        load_local(cx, "image with spaces.png", &source),
        "a literal space path still works"
    );
    assert!(
        load_local(cx, "中文名.png", &source),
        "a literal CJK path still works"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn literal_percent_filenames_survive_decoding(cx: &mut gpui::TestAppContext) {
    let dir = fixture_dir("literal");
    write_png(&dir.join("100%.png"));
    let source = dir.join("document.md");
    std::fs::write(&source, "").unwrap();

    assert!(
        load_local(cx, "100%.png", &source),
        "an invalid escape (`%.p`) is not %XX; the decoder must keep it verbatim"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn http_throttling_remains_retryable(cx: &mut gpui::TestAppContext) {
    let future = cx.update(|app| {
        app.set_http_client(status_client(429, Some("1")));
        images::load_source(
            images::Resolved::Remote("https://example.invalid/image.png".to_owned()),
            true,
            app,
        )
    });
    let result = futures::executor::block_on(future);
    assert!(
        matches!(result, Err(SourceError::Retryable { .. })),
        "429 rate limiting must go through backoff retry, not permanent failure: {result:?}"
    );
}

#[gpui::test]
fn http_request_timeout_remains_retryable(cx: &mut gpui::TestAppContext) {
    let future = cx.update(|app| {
        app.set_http_client(status_client(408, None));
        images::load_source(
            images::Resolved::Remote("https://example.invalid/image.png".to_owned()),
            true,
            app,
        )
    });
    let result = futures::executor::block_on(future);
    assert!(
        matches!(result, Err(SourceError::Retryable { .. })),
        "408 timeout must go through backoff retry: {result:?}"
    );
}

#[gpui::test]
fn http_not_found_stays_fatal(cx: &mut gpui::TestAppContext) {
    let future = cx.update(|app| {
        app.set_http_client(status_client(404, None));
        images::load_source(
            images::Resolved::Remote("https://example.invalid/image.png".to_owned()),
            true,
            app,
        )
    });
    let result = futures::executor::block_on(future);
    assert!(
        matches!(result, Err(SourceError::Fatal(_))),
        "retrying a 404 just gets the same answer again; stay Fatal: {result:?}"
    );
}
