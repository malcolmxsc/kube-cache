# 🚀 Kube-Cache: The GPU Data Pre-Warmer

**Status:** 🚧 In Development (Phase 1 Complete)
**Tech Stack:** Rust, Kubernetes (Kind), Tokio, Kube-rs

## 💡 The Problem
In AI/ML infrastructure, GPU idle time is the most expensive resource leak. When a training job starts, it often sits idle at 0% GPU utilization while waiting for terabytes of training data to download from S3.

## 🛠 The Solution
**Kube-Cache** is a custom Kubernetes Operator that intercepts Pod scheduling. It pauses the GPU Pod, pre-fetches the required dataset to a local NVMe cache on the node, and *then* releases the Pod.

**Result:** 0% GPU Idle time. Training starts instantly.

## 🗺 Architecture & Roadmap
### ✅ Phase 1: The Foundation
- [x] Set up local development environment (Kind Cluster with simulated GPUs).
- [x] Implement Rust-based connection to Kubernetes API.
- [x] Successfully list and monitor existing Pods.

### 🔜 Phase 2: The Watcher
- [ ] Upgrade Operator to "Watch Mode" (Stream API).
- [ ] Detect Pods with specific `x-openai/required-dataset` annotations.

### 🔮 Phase 3: The Interceptor
- [ ] Implement MutatingAdmissionWebhook to "pause" Pods.
- [ ] Logic to check if data exists on the node.

### 🚀 Phase 4: The Action
- [ ] Spawn "Data Fetcher" jobs to pull from S3/MinIO.
- [ ] Mount hostPath volumes to the GPU Pod.

## 💻 How to Run (Dev)
1. `kind create cluster --config infrastructure/kind-config.yaml`
2. `cargo run`
