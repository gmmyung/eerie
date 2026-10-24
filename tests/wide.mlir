// Keep calls in VM IR so the compiler cannot fold away the register copies
// covered by IREE #24946. Each value has distinct nonzero high and low words.
vm.module @wide {
  vm.export @check
  vm.func @check() {
    %a = vm.const.i64 4294967298
    %b = vm.const.i64 12884901892
    %x, %y = vm.call @pair(%a, %b) : (i64, i64) -> (i64, i64)
    %bad_a = vm.cmp.ne.i64 %a, %x : i64
    %bad_b = vm.cmp.ne.i64 %b, %y : i64
    %bad = vm.or.i32 %bad_a, %bad_b : i32
    vm.cond_br %bad, ^fail, ^pass
  ^fail:
    %status = vm.const.i32 9
    vm.fail %status, "internal i64 call lost high words"
  ^pass:
    vm.return
  }
  vm.func private @pair(%a: i64, %b: i64) -> (i64, i64)
      attributes {inlining_policy = #util.inline.never} {
    vm.return %a, %b : i64, i64
  }
}
