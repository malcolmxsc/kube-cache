# ⚡ Kube-Cache: The GPU Data Pre-Warmer

**Status:** ✅ Complete (v1.0)  
**Tech Stack:** Rust, Kubernetes (Kind), Tokio, Kube-rs

## 💡 The Problem: The "GPU Tax"
In deep learning infrastructure, the most expensive resource is GPU time (approx. $4/hr for an H100). 
When a training Pod starts, it typically spends the first 10-30 minutes initializing and downloading terabytes of datasets from object storage (S3/GCS).

**The Result:** The GPU sits idle at 0% utilization while the meter is running. This "Data Loading Tax" wastes millions of dollars annually at scale.

## 🛠 The Solution
**Kube-Cache** is a Kubernetes Operator written in Rust that eliminates this idle time using the **Delegation Pattern**.

It intercepts Pods before they are scheduled, delegates the data fetching to a cheap CPU-based worker, and only schedules the GPU Pod once the data is locally available on NVMe.

### Architecture
[User submits Pod] 
       ⬇
[🚫 Scheduling Gate] (Pod is stuck in Pending)
       ⬇
[🤖 Kube-Cache Operator] (Detects Gated Pod)
       ⬇
[🚜 Spawns Fetcher Job] (Cheap CPU Worker downloads S3 data to hostPath)
       ⬇
[✅ Job Complete] 
       ⬇
[🔓 Gate Removed] (Pod is released to Scheduler)
       ⬇
[🚀 GPU Pod Starts] (Data is already on disk. Training starts instantly.)

## 🚀 Features Implemented
- **Rust-based Operator:** Uses `kube-rs` for high-performance, type-safe control loops.
- **Scheduling Gates:** leverages K8s v1.26+ API to non-destructively pause pod scheduling.
- **Job Delegation:** Spawns ephemeral `batch/v1` Jobs to handle data transfer, keeping the Operator lightweight.
- **Smart Node Targeting:** Ensures the Fetcher Job runs on the exact same node that the GPU Pod will eventually occupy.
- **Contract-Based Trigger:** Activates only when the `x-openai/required-dataset` annotation is present.

<<<<<<< Updated upstream
### 🔮 Phase 3: The Interceptor
- [x] Implement MutatingAdmissionWebhook to "pause" Pods.
- [x] Logic to check if data exists on the node.

### 🚀 Phase 4: The Action
- [x] Spawn "Data Fetcher" jobs to pull from S3/MinIO.
- [x] Mount hostPath volumes to the GPU Pod.

## 💻 How to Run (Dev)
1. `kind create cluster --config infrastructure/kind-config.yaml`
2. `cargo run`
=======
## 💻 How to Run (Local Simulation)

### 1. Start the Cluster
```bash
kind create cluster --config infrastructure/kind-config.yaml
```
>>>>>>> Stashed changes
