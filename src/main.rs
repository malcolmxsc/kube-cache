// --- IMPORTS ---
// We are bringing in specific tools from the libraries we installed.
use kube::{Api, Client, api::{WatchEvent, WatchParams}};
use futures::StreamExt;


use k8s_openapi::api::core::v1::Pod;


// --- THE MAIN FUNCTION ---
// The attribute #[tokio::main] is a "Macro".
// Rust's default 'main' function cannot be asynchronous (it can't wait for network requests).
// This macro wraps our function in a runtime that CAN wait for network requests.
#[tokio::main]

async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    // 1. Initialize Logging
    // This turns on the printing tools so we can see output in the terminal.
    tracing_subscriber::fmt::init();

    println!("🚀 Kube-Cache Operator is starting...");

    // 2. Connect to Kubernetes
    // 'let' defines a variable.
    // 'Client::try_default()' tries to find your ~/.kube/config file.
    // '.await' means: "Pause this line until the connection is ready."
    // '?' means: "If this FAILS, stop the program and print the error. If it SUCCEEDS, give me the client."
    let client = Client::try_default().await?;

    // 3. Define the API we want to talk to
    // We want to talk to 'Pods' in the 'kube-system' namespace.
    // The syntax '<Pod>' is a Generic. It tells the compiler: "This API expects Pod objects, not Services or Nodes."
    let pods: Api<Pod> = Api::namespaced(client, "default");


    
    // 1. create the Watch stream.
    // "0" is the resource version. it means give me all the events starting now.
    // .boxed() just wraps the complicated type into a box.
    let wp = WatchParams::default();
    println!("👀 Watching for Pods in 'default' namespace...");
    let mut stream = pods.watch(&wp, "0").await?.boxed();

    // 2. the infinite loop
    // while let  is a loop that runs as long as the stream is open.
    while let Some(status) = stream.next().await {
        // if a pod is modified, or added. 
        match status {
        Ok(WatchEvent::Added(pod)) | Ok(WatchEvent::Modified(pod)) => {
            let name = pod.metadata.name.clone().unwrap_or_default();
           // check if the pod has annotations (metadata)
           if let Some(annotations) = pod.metadata.annotations {
            // 2. check if it is our one specific  "contract" annotation
            if let Some(dataset_url) = annotations.get("x-openai/required-dataset") {
                println!("TRIGGER: Pod '{}' needs data from: {}",name, dataset_url);
                // this is where we will trigger the download of the dataset.
                
            }
           }
           
        },
        // if we lose the connection
        Ok(WatchEvent::Error(e)) => println!("Error: {}", e),
        // ignore other events like de.
        _ => {}

    }
    }
    Ok(())
}