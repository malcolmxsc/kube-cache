# Project Roadmap

## 1. Predictive Pre-caching (The "System Intelligence" Layer)

### Problem
Inference latency and cold starts can degrade user experience during sudden traffic spikes. Waiting for Pods to be scheduled and containers to start creates a lag between demand and availability.

### Solution
Implement a proactive caching mechanism that leverages historical data and real-time metrics. Instead of reacting to Pending pods, the operator will query Prometheus to detect emerging traffic patterns (e.g., "Monday 9am Login Rush").

### Implementation Plan
- **Traffic Analysis**: Integrate with Prometheus to monitor ingress traffic rates and identify trends.
- **Predictive Trigger**: define thresholds where the operator triggers pre-warming events on idle nodes *before* the Horizontal Pod Autoscaler (HPA) requests new replicas.
- **Pre-warming Logic**: The operator will instruct idle nodes to pull model weights, ensuring zero-latency scale-up when the actual Pods are scheduled.

## 2. P2P Weight Distribution (The "Hyper-Scale" Layer)

### Problem
Scaling to 1,000+ GPUs creates a "Thundering Herd" problem where all nodes simultaneously attempt to download large model weights from S3. This leads to S3 saturation, throttling, and massive egress costs.

### Solution
Prevent S3 saturation by implementing a BitTorrent-style peer-to-peer distribution layer. Nodes will share cached data amongst themselves over the high-speed internal cluster network.

### Implementation Plan
- **Peer Discovery**: Implement a Gossip protocol (e.g., utilizing `memberlist`) for nodes to discover peers within the cluster.
- **P2P Transfer**: Modify the download logic so that if Node A already has the required weights, Node B will download directly from Node A instead of reaching out to S3.
- **Fallback Mechanism**: Maintain S3 as the authoritative source if no peers have the data or if P2P transfer fails.
