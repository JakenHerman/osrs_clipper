# OSRS Clipper

<img src="https://external-content.duckduckgo.com/iu/?u=https%3A%2F%2Fwww.pngitem.com%2Fpimgs%2Fm%2F121-1219619_gnome-child-png-runescape-gnome-child-transparent-png.png&f=1&nofb=1&ipt=097263fd0d734cfb819ba235435dc52b07f0557d42d386a5ea5598ed5365437f&ipo=images" />

Our goal is to build an OSRS RuneScape clipping tool that does the following:

 - [ ] Detect Key Moments: Analyze an OSRS video stream to identify events like loot drops, PvP kills, or level-ups using computer vision or AI.

 - [ ] Generate Highlight Clips: When a key event is detected, automatically generate a video clip from the stream using tools like FFmpeg.

 - [ ] Store Clips on S3: Upload the generated clip to an Amazon S3 bucket for persistent storage.

 - [ ] Provide a Web API: Create an API using Actix Web to retrieve a list of clips and provide access to each clip’s URL.

 - [ ] Distributed Processing on EKS: Package the application as Docker containers and run them on AWS EKS for scalability.

``` mermaid
flowchart TD
    A[OSRS Video Stream] --> B[Video Ingestion Service]
    B --> C[Frame Extraction & AI Detection]
    C --> D["Clip Generation (FFmpeg)"]
    D --> E["S3 Storage (AWS SDK)"]
    E --> F["Web API Service (Actix Web)"]
    
    subgraph AWS EKS
      B
      C
      D
      F
    end
```

### Setup
Start MinIO:
```
minio server ./data --console-address ":9001"
```

Create a `.env` with the following:
```
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
AWS_REGION=us-east-1
S3_ENDPOINT=http://127.0.0.1:9000
S3_BUCKET=clips
```
