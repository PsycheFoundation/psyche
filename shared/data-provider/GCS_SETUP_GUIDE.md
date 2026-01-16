# Google Cloud Storage Bucket creation guide

## What You'll Need

- A Google account
- A credit or debit card for billing purposes

---

## Step 1: Go to Google Cloud Console

1. Open your browser and go to [console.cloud.google.com](https://console.cloud.google.com)
2. Sign in with your Google account

---

## Step 2: Create a Project

A project is a container for all your Google Cloud resources.

1. Click the project dropdown at the top of the page (it may say "Select a project" or show an existing project name)
2. Click **New Project** in the top right of the popup
3. Enter a **Project name** (e.g., `my-ml-checkpoints`)
4. Leave Organization as "No organization" (unless you have one)
5. Click **Create**
6. Wait a few seconds, then select your new project from the dropdown

---

## Step 3: Set Up Billing

Google Cloud requires a billing account to use any services, even free ones.

1. Click the hamburger menu (☰) in the top left
2. Go to **Billing**
3. Click **Link a billing account** (or **Create account** if you don't have one)

### If creating a new billing account:

1. Click **Create billing account**
2. Choose your country
3. Enter your payment information (credit or debit card)
4. Click **Submit and enable billing**

Your project is now linked to your billing account.

> **Note:** Google Cloud has a free tier. Small buckets with light usage cost almost nothing. You can also set up budget alerts to avoid surprises.

---

## Step 4: Create a Storage Bucket

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
- **Object versioning**: Turn **ON** — this is important for having a history of the latest checkpoints. It keeps previous versions when files are overwritten.
  Select a number of versions to store per object – this will be important so that storage of the bucket doesn't grow infinitely. Set a reasonable number depending on the amount of checkpoints you want stored.
  Leave the 'Expire noncurrent versions after' blank so that old versions of the checkpoints are not deleted after some amount of time.

9. Encryption – Leave as **Google-managed encryption key** (default)
10. Click **Create**. If prompted, leave "enforce public access prevention" **on**

---

## Step 5: Verify Your Bucket

1. You should see your new bucket in the list
2. Click on the bucket name to open it
3. You can now upload files using the **Upload files** button

---

## Step 6: Grant Storage Access to a Gmail Account

To allow users to access to the bucket in order to push checkpoints in a training run, you can grant them bucket-level permissions.

1. Go to **Cloud Storage** → **Buckets**
2. Click on your bucket name to open it
3. Click the **Permissions** tab
4. Click **Grant Access**
5. In the **New principals** field, enter the Gmail address (e.g., `someone@gmail.com`)
6. Click **Select a role** and choose **Cloud Storage** → **Storage Object User**. This allows read, list, create, and overwrite objects, but not delete
7. Click **Save**

The user can now authenticate using the gcloud CLI. If you don't have it installed, follow the [installation guide](https://cloud.google.com/sdk/docs/install).

```
gcloud auth application-default login
```

or

```
gcloud auth application-default login --scopes="https://www.googleapis.com/auth/cloud-platform"
```

from their machine.

---

## Useful Links

- [Google Cloud Console](https://console.cloud.google.com)
- [Cloud Storage Documentation](https://cloud.google.com/storage/docs)
- [Pricing Calculator](https://cloud.google.com/products/calculator)
- [Free Tier Details](https://cloud.google.com/free)
