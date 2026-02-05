# GCS Checkpoints

Psyche supports uploading training checkpoints to Google Cloud Storage (GCS). This page covers how to set up a GCS bucket for checkpoints and explains the manifest system used to ensure reliable checkpoint downloads.

## Setting Up a GCS Bucket

### Prerequisites

- A Google account
- A credit or debit card for billing purposes

### Step 1: Go to Google Cloud Console

1. Open your browser and go to [console.cloud.google.com](https://console.cloud.google.com)
2. Sign in with your Google account

### Step 2: Create a Project

A project is a container for all your Google Cloud resources.

1. Click the project dropdown at the top of the page (it may say "Select a project" or show an existing project name)
2. Click **New Project** in the top right of the popup
3. Enter a **Project name** (e.g., `my-ml-checkpoints`)
4. Leave Organization as "No organization" (unless you have one)
5. Click **Create**
6. Wait a few seconds, then select your new project from the dropdown

### Step 3: Set Up Billing

Google Cloud requires a billing account to use any services, even free ones.

1. Click the hamburger menu (☰) in the top left
2. Go to **Billing**
3. Click **Link a billing account** (or **Create account** if you don't have one)

If creating a new billing account:

1. Click **Create billing account**
2. Choose your country
3. Enter your payment information (credit or debit card)
4. Click **Submit and enable billing**

Your project is now linked to your billing account.

> **Note:** Google Cloud has a free tier. Small buckets with light usage cost almost nothing. You can also set up budget alerts to avoid surprises.

### Step 4: Create a Storage Bucket

1. Click the hamburger menu (☰) in the top left
2. Scroll down and click **Cloud Storage** → **Buckets**
3. Once you are redirected to the Buckets workspace, click **Create** at the top
4. Enter a **globally unique** name (e.g., `yourname-ml-checkpoints-2025`)
5. Choose a location type:
   - **Region** (recommended): Cheapest option. Best for backups, large files, or when your compute runs in a single region. Pick one close to you (e.g., `us-central1`, `europe-west1`).
   - **Multi-region**: ~20-30% more expensive. Better availability and lower latency for users spread globally. Choose this if you're serving content worldwide and need fast access.
   - **Dual-region**: Most expensive. High availability between two specific regions. Rarely needed for most use cases.
6. Choose where to store your data - select the **Standard** storage class. This is best for frequently accessed data
7. Choose how to store your data - select **Uniform** access control and leave "Enforce public access prevention" checked
8. Choose how to protect object data:
   - **Soft delete**: Leave default (7 days) — lets you recover accidentally deleted files
   - **Object versioning**: Turn **ON** — this is important for having a history of the latest checkpoints. It keeps previous versions when files are overwritten. Select a number of versions to store per object – this will be important so that storage of the bucket doesn't grow infinitely. Set a reasonable number depending on the amount of checkpoints you want stored. Leave the 'Expire noncurrent versions after' blank so that old versions of the checkpoints are not deleted after some amount of time.
9. Encryption – Leave as **Google-managed encryption key** (default)
10. Click **Create**. If prompted, leave "enforce public access prevention" **on**

### Step 5: Verify Your Bucket

1. You should see your new bucket in the list
2. Click on the bucket name to open it
3. You can now upload files using the **Upload files** button

### Step 6: Grant Storage Access to Users

To allow users to access the bucket in order to push checkpoints in a training run, you can grant them bucket-level permissions.

1. Go to **Cloud Storage** → **Buckets**
2. Click on your bucket name to open it
3. Click the **Permissions** tab
4. Click **Grant Access**
5. In the **New principals** field, enter the Gmail address (e.g., `someone@gmail.com`)
6. Click **Select a role** and choose **Cloud Storage** → **Storage Object User**. This allows read, list, create, and overwrite objects, but not delete
7. Click **Save**

The user can now authenticate using the gcloud CLI. If you don't have it installed, follow the [installation guide](https://cloud.google.com/sdk/docs/install).

```bash
gcloud auth application-default login
```

or

```bash
gcloud auth application-default login --scopes="https://www.googleapis.com/auth/cloud-platform"
```

This creates a credentials file at `~/.config/gcloud/application_default_credentials.json`. To use these credentials with the run-manager, set the `GOOGLE_CREDENTIALS_FILE_PATH` variable in your env file:

```bash
GOOGLE_CREDENTIALS_FILE_PATH=/home/youruser/.config/gcloud/application_default_credentials.json
```

The run-manager will mount this file into the Docker container and configure the `GOOGLE_APPLICATION_CREDENTIALS` environment variable automatically.

### Useful Links

- [Google Cloud Console](https://console.cloud.google.com)
- [Cloud Storage Documentation](https://cloud.google.com/storage/docs)
- [Pricing Calculator](https://cloud.google.com/products/calculator)
- [Free Tier Details](https://cloud.google.com/free)

---

## Checkpoint Manifest

The `manifest.json` file is a metadata document uploaded to GCS alongside checkpoint files. It serves as an atomic, versioned index of checkpoint files that enables reliable and efficient checkpoint downloads.

### File Location

```
gs://{bucket}/{prefix}/manifest.json
```

Or without prefix:

```
gs://{bucket}/manifest.json
```

### Schema

```json
{
	"metadata": {
		"timestamp": "2024-01-15T10:30:00Z",
		"epoch": 5,
		"step": 12500,
		"run_id": "my-training-run"
	},
	"files": [
		{
			"filename": "model-00001-of-00004.safetensors",
			"generation": 1705312200123456,
			"size_bytes": 536870912
		},
		{
			"filename": "model-00002-of-00004.safetensors",
			"generation": 1705312205654321,
			"size_bytes": 536870912
		}
	]
}
```

| Field                | Description                                 |
| -------------------- | ------------------------------------------- |
| `metadata.timestamp` | ISO 8601 timestamp of upload                |
| `metadata.epoch`     | Training epoch number                       |
| `metadata.step`      | Training step number                        |
| `metadata.run_id`    | Unique identifier for the training run      |
| `files[].filename`   | Name of the checkpoint file                 |
| `files[].generation` | GCS object generation number for versioning |
| `files[].size_bytes` | File size in bytes                          |

### Why Use a Manifest?

#### 1. Atomic Checkpoint Consistency

GCS has eventual consistency for object listings. When a checkpoint consists of multiple safetensors shards (e.g., `model-00001-of-00004.safetensors` through `model-00004-of-00004.safetensors`), a bucket listing might return a mix of files from different checkpoint versions if an upload is in progress.

The manifest is uploaded **after** all safetensors files are successfully uploaded. It acts as an atomic marker indicating that all files for a checkpoint are available. Downloaders read the manifest first to get the exact list of files to fetch.

#### 2. GCS Object Versioning with Generation Numbers

GCS uses "generation numbers" to version objects. When a file is overwritten, it gets a new generation number. Without tracking generations, a client might download file A from checkpoint v1 and file B from checkpoint v2 if uploads overlap.

The manifest records the exact `generation` number for each file at upload time. During download, the client requests files with their specific generation numbers, ensuring all files belong to the same checkpoint version.

```rust
// Upload: Record generation after each file upload
manifest.files.push(ManifestFileEntry {
    filename: file_name.to_string(),
    generation: uploaded.generation,  // GCS returns this after upload
    size_bytes: size,
});

// Download: Request specific generation
client.download_object(&GetObjectRequest {
    generation: Some(file_entry.generation),  // Pin to exact version
    ..
})
```

### Upload Flow

1. Client completes a training epoch and saves checkpoint locally
2. `upload_to_gcs()` is called with local file paths
3. For each `.safetensors` file:
   - Upload to GCS
   - Capture the returned `generation` number
   - Add entry to manifest's `files` array
4. Serialize manifest to JSON and upload to `{prefix}/manifest.json`
5. Notify coordinator via channel with `Checkpoint::Gcs(GcsRepo { bucket, prefix })`

### Download Flow

1. Client receives `Checkpoint::Gcs` from coordinator
2. Fetch `manifest.json` metadata to get its generation number
3. Download and parse manifest JSON
4. Compute cache directory: `step-{step}-{manifest_generation}`
5. Check if all files exist in cache
   - **Cache hit:** Return cached file paths immediately
   - **Cache miss:** Download each file using its recorded generation number
6. Also download config files (`.json`, `.py`) that aren't in manifest
7. Return list of local file paths

### Files Tracked

The manifest only tracks `.safetensors` files (model weights). Config files (`.json`, `.py`) are downloaded separately via bucket listing because:

- They're small and change infrequently
- They may be shared across checkpoints
- They don't have the same consistency concerns as sharded model weights
