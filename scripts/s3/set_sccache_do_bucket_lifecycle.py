#
# This script sets the bucket lifecycle policy of the sccache digitalocean
# space backend to automatically delete records after a period of time.
#
# to run this script, DO space key and secret need to be configured with
# environment variables `DO_SPACE_KEY` and `DO_SPACE_SECRET` respectively.
#

import boto3
import json
import os


def main():
    bucket = "forest-sccache-us-west"
    lifecycle_config = {
        "Rules": [
            {
                "Expiration": {
                    "Days": 30,
                },
                "ID": "cache-retention",
                "Prefix": "",
                "Status": "Enabled",
            },
        ]
    }
    s3 = boto3.client(
        "s3",
        region_name="sfo3",
        endpoint_url="https://sfo3.digitaloceanspaces.com",
        aws_access_key_id=os.getenv("DO_SPACE_KEY"),
        aws_secret_access_key=os.getenv("DO_SPACE_SECRET"),
    )
    s3.put_bucket_lifecycle_configuration(
        Bucket=bucket, LifecycleConfiguration=lifecycle_config
    )
    result = s3.get_bucket_lifecycle_configuration(Bucket=bucket)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
