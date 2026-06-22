module @bools {
  func.func @logical_not(%arg0: tensor<4xi1>) -> tensor<4xi1> {
    %true = arith.constant dense<true> : tensor<4xi1>
    %0 = arith.xori %arg0, %true : tensor<4xi1>
    return %0 : tensor<4xi1>
  }
}
