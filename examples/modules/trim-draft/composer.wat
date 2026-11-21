(module
  (memory (export "memory") 1 1)
  (func $whitespace (param $byte i32) (result i32)
    (i32.or
      (i32.eq (local.get $byte) (i32.const 32))
      (i32.le_u (i32.sub (local.get $byte) (i32.const 9)) (i32.const 4))))
  (func (export "transform") (param $end i32) (result i32)
    (local $start i32)
    (block $found_start
      (loop $leading
        (br_if $found_start (i32.ge_u (local.get $start) (local.get $end)))
        (br_if $found_start
          (i32.eqz (call $whitespace (i32.load8_u (local.get $start)))))
        (local.set $start (i32.add (local.get $start) (i32.const 1)))
        (br $leading)))
    (block $found_end
      (loop $trailing
        (br_if $found_end (i32.le_u (local.get $end) (local.get $start)))
        (br_if $found_end
          (i32.eqz
            (call $whitespace
              (i32.load8_u (i32.sub (local.get $end) (i32.const 1))))))
        (local.set $end (i32.sub (local.get $end) (i32.const 1)))
        (br $trailing)))
    (local.set $end (i32.sub (local.get $end) (local.get $start)))
    (memory.copy (i32.const 0) (local.get $start) (local.get $end))
    (local.get $end)))
