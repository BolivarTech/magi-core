let r = call().map_err(|e| to_provider_error("op", &redacted, &e));
