fn main() {
    println!("Testing basic imports...");
    
    // 只测试最基本的类型是否存在
    let _ = std::marker::PhantomData::<msgtrans::TransportConfig>;
    
    println!("Basic import works!");
} 