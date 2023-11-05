module @arithmetic {
  func.func @simple_mul(%arg0: tensor<100xf32>, %arg1: tensor<100xf32>) -> tensor<100xf32> {
    %0 = arith.mulf %arg0, %arg1 : tensor<100xf32>
    return %0 : tensor<100xf32>
  }
}
