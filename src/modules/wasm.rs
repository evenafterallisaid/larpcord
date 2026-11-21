use wasmi::{Config, EnforcedLimits, Engine, Linker, Module, Store, StoreLimitsBuilder, TrapCode};

pub(super) const MAX_BINARY: usize = 256 * 1024;
pub(crate) const MAX_DRAFT: usize = 16 * 1024;

pub(super) fn transform(bytes: &[u8], draft: &str) -> Result<String, &'static str> {
    if draft.len() > MAX_DRAFT {
        return Err("Draft is too long for this tool (16 KiB limit).");
    }
    let mut config = Config::default();
    config
        .consume_fuel(true)
        .allow_start_fn(false)
        .ignore_custom_sections(true)
        .enforced_limits(EnforcedLimits::strict())
        .set_max_recursion_depth(96)
        .set_max_stack_height(256 * 1024);
    let engine = Engine::new(&config);
    let module =
        Module::new(&engine, bytes).map_err(|_| "This Wasm module could not be loaded.")?;
    if module.imports().next().is_some() {
        return Err("Composer modules cannot import host functions or memory.");
    }
    let mut store = Store::new(
        &engine,
        StoreLimitsBuilder::new()
            .memory_size(2 * 1024 * 1024)
            .memories(1)
            .tables(1)
            .table_elements(1024)
            .instances(1)
            .trap_on_grow_failure(true)
            .build(),
    );
    store.limiter(|limits| limits);
    store
        .set_fuel(500_000)
        .map_err(|_| "Could not start the composer tool.")?;
    let instance = Linker::new(&engine)
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| "This Wasm module could not be started within its limits.")?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or("Missing the composer memory export.")?;
    let tool = instance
        .get_typed_func::<i32, i32>(&store, "transform")
        .map_err(|_| "Missing a compatible transform export.")?;
    memory
        .write(&mut store, 0, draft.as_bytes())
        .map_err(|_| "The module has too little memory for this draft.")?;
    let length = tool.call(&mut store, draft.len() as i32).map_err(|error| {
        if error.as_trap_code() == Some(TrapCode::OutOfFuel) {
            "The composer tool exceeded its time budget."
        } else {
            "The composer tool stopped before returning a result."
        }
    })?;
    let length = usize::try_from(length)
        .ok()
        .filter(|len| *len <= MAX_DRAFT)
        .ok_or("The composer tool returned an invalid draft length.")?;
    let mut output = vec![0; length];
    memory
        .read(&store, 0, &mut output)
        .map_err(|_| "The composer tool returned an invalid draft.")?;
    String::from_utf8(output)
        .ok()
        .filter(|text| {
            !text
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\t' | '\n' | '\r'))
        })
        .ok_or("The composer tool did not return plain UTF-8 text.")
}

#[cfg(test)]
mod tests {
    use super::transform;

    #[test]
    fn interpreter_stops_a_nonreturning_transform() {
        // One exported memory and a transform consisting of loop { br 0 }.
        let looping = b"\0asm\x01\0\0\0\x01\x06\x01\x60\x01\x7f\x01\x7f\x03\x02\x01\0\x05\x04\x01\x01\x01\x01\x07\x16\x02\x06memory\x02\0\x09transform\0\0\x0a\x0a\x01\x08\0\x03\x40\x0c\0\x0b\0\x0b";
        assert_eq!(
            transform(looping, "draft"),
            Err("The composer tool exceeded its time budget.")
        );
    }
}
