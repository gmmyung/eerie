module @mixed {
  func.func @is_positive(%arg0: tensor<4xf32>) -> tensor<4xi1> {
    %zero = arith.constant dense<0.0> : tensor<4xf32>
    %0 = arith.cmpf ogt, %arg0, %zero : tensor<4xf32>
    return %0 : tensor<4xi1>
  }

  func.func @select_masked(%values: tensor<4xf32>, %mask: tensor<4xi1>) -> tensor<4xf32> {
    %zero = arith.constant dense<0.0> : tensor<4xf32>
    %0 = arith.select %mask, %values, %zero : tensor<4xi1>, tensor<4xf32>
    return %0 : tensor<4xf32>
  }
}
